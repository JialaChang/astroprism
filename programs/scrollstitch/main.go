// Author: JialaChang

// scrollstitch stitches scroll-recording frames into one tall PNG by matching their vertical overlap.

package main

import (
	"fmt"
	"image"
	"image/draw"
	"image/png"
	"io"
	"math"
	"os"
	"sort"
	"strconv"
)

const (
	minShift     = 4
	maxShiftFrac = 0.8

	// Max mean squared profile error; real scrolls score < 30, bogus matches > 70.
	errThreshold = 50.0
)

type frameProfile struct {
	mean []float64 // per-row average luma
	edge []float64 // per-row horizontal total variation (L1), a robust edge-density proxy
}

func absInt(x int) int {
	if x < 0 {
		return -x
	}
	return x
}

// readFrame reads one raw RGBA frame (width*height*4 bytes).
// It returns io.EOF once the stream is exhausted after a complete frame.
func readFrame(r io.Reader, width, height int) (*image.RGBA, error) {
	buf := make([]byte, width*height*4)
	if _, err := io.ReadFull(r, buf); err != nil {
		return nil, err
	}
	return &image.RGBA{
		Pix:    buf,
		Stride: width * 4,
		Rect:   image.Rect(0, 0, width, height),
	}, nil
}

// rowProfile computes per-row mean luma and horizontal total variation.
func rowProfile(img *image.RGBA) frameProfile {
	b := img.Bounds()
	w, h := b.Dx(), b.Dy()
	profile := frameProfile{
		mean: make([]float64, h),
		edge: make([]float64, h),
	}
	for y := 0; y < h; y++ {
		row := img.Pix[y*img.Stride : y*img.Stride+w*4]
		var lumaSum, edgeSum, prevLuma int
		for x := 0; x < w; x++ {
			rch := row[x*4]
			gch := row[x*4+1]
			bch := row[x*4+2]
			// Rec. 709 luma in 8-bit fixed point: 54/183/19 ~= 0.2126/0.7152/0.0722 * 256
			luma := 54*int(rch) + 183*int(gch) + 19*int(bch)
			lumaSum += luma
			if x > 0 {
				d := luma - prevLuma
				edgeSum += absInt(d)
			}
			prevLuma = luma
		}
		profile.mean[y] = float64(lumaSum) / float64(w*256)
		// x==0 has no left neighbor, so edgeSum holds w-1 samples
		profile.edge[y] = float64(edgeSum) / float64((w-1)*256)
	}
	return profile
}

const (
	ratioMargin = 20  // exclude rows around the winner to find the runner-up
	ratioMax    = 0.8 // max acceptable ratio (best / second-best)
)

// bestOffsetExcluding returns the lowest-error offset, skipping excludeRadius around excludeCenter (-1 = none).
func bestOffsetExcluding(prev, curr frameProfile, excludeCenter, excludeRadius int) (offset int, score float64) {
	h := len(prev.mean)
	maxShift := int(float64(h) * maxShiftFrac)
	bestOffset := 0
	bestErr := math.MaxFloat64
	for offset := minShift; offset <= maxShift; offset++ {
		if excludeCenter >= 0 && absInt(offset-excludeCenter) <= excludeRadius {
			continue
		}
		n := h - offset
		if n <= 0 {
			continue
		}
		// offset is doomed once errSum exceeds bestErr*n since errSum only grows
		limit := bestErr * float64(n)
		var errSum float64
		abandoned := false
		for i := 0; i < n; i++ {
			dm := prev.mean[offset+i] - curr.mean[i]
			de := prev.edge[offset+i] - curr.edge[i]
			errSum += dm*dm + de*de
			if errSum > limit {
				abandoned = true
				break
			}
		}
		if abandoned {
			continue
		}
		avgErr := errSum / float64(n)
		if avgErr < bestErr {
			bestErr = avgErr
			bestOffset = offset
		}
	}
	return bestOffset, bestErr
}

// findOffset finds how far content scrolled from prev to curr.
// ok=false when a distinct runner-up scores nearly as well (self-similar content).
func findOffset(prev, curr frameProfile) (offset int, score float64, ok bool) {
	bestOffset, bestErr := bestOffsetExcluding(prev, curr, -1, 0)
	if bestErr == math.MaxFloat64 {
		return 0, bestErr, false
	}
	_, secondErr := bestOffsetExcluding(prev, curr, bestOffset, ratioMargin)
	if secondErr == math.MaxFloat64 {
		return bestOffset, bestErr, true
	}
	if secondErr == 0 || bestErr/secondErr > ratioMax {
		return bestOffset, bestErr, false
	}
	return bestOffset, bestErr, true
}

const (
	verifyRowStep   = 8
	verifyColStep   = 16
	verifyThreshold = 12.0 // max mean abs green diff in the overlap region

	// Same picture below this; repeats score < 0.5, real scrolls > 3.
	staticThreshold = 1.0
)

// frameChanged reports whether anything moved; repeats would still match near the ceiling.
func frameChanged(prev, curr *image.RGBA) bool {
	w := prev.Bounds().Dx()
	h := prev.Bounds().Dy()
	var sum, count int
	for y := 0; y < h; y += verifyRowStep {
		prow := prev.Pix[y*prev.Stride:]
		crow := curr.Pix[y*curr.Stride:]
		for x := 0; x < w; x += verifyColStep {
			sum += absInt(int(prow[x*4+1]) - int(crow[x*4+1]))
			count++
		}
	}
	return float64(sum)/float64(count) > staticThreshold
}

// verifyOffset checks an offset against pixels, since 1D profiles can collide.
func verifyOffset(prev, curr *image.RGBA, offset int) bool {
	w := prev.Bounds().Dx()
	h := prev.Bounds().Dy()
	n := h - offset
	var sum, count int
	for y := 0; y < n; y += verifyRowStep {
		prow := prev.Pix[(y+offset)*prev.Stride:]
		crow := curr.Pix[y*curr.Stride:]
		for x := 0; x < w; x += verifyColStep {
			d := int(prow[x*4+1]) - int(crow[x*4+1])
			sum += absInt(d)
			count++
		}
	}
	if count == 0 {
		return false
	}
	return float64(sum)/float64(count) <= verifyThreshold
}

// Changed frames held back to locate fixed rows before stitching starts.
const bandWarmup = 8

// band is the scrolling content [top, bottom); rows outside it stay put (sticky header/footer).
type band struct{ top, bottom int }

// rowsUnchanged reports, per row, whether it is the same at the same position in both frames.
func rowsUnchanged(prev, curr *image.RGBA) []bool {
	w := prev.Bounds().Dx()
	h := prev.Bounds().Dy()
	same := make([]bool, h)
	for y := 0; y < h; y++ {
		prow := prev.Pix[y*prev.Stride:]
		crow := curr.Pix[y*curr.Stride:]
		var sum int
		for x := 0; x < w; x++ {
			sum += absInt(int(prow[x*4+1]) - int(crow[x*4+1]))
		}
		same[y] = float64(sum)/float64(w) <= staticThreshold
	}
	return same
}

// detectBand finds the fixed header/footer rows. Overestimating is harmless; a band under half the frame is distrusted.
func detectBand(frames []*image.RGBA) band {
	h := frames[0].Bounds().Dy()
	full := band{0, h}
	if len(frames) < 2 {
		return full
	}
	changed := make([]int, h)
	for i := 1; i < len(frames); i++ {
		for y, same := range rowsUnchanged(frames[i-1], frames[i]) {
			if !same {
				changed[y]++
			}
		}
	}
	// A coarse keyframe disturbs fixed rows in the odd pair, so a quarter may dissent.
	allowed := (len(frames) - 1) / 4
	top := 0
	for top < h && changed[top] <= allowed {
		top++
	}
	bottom := h
	for bottom > top && changed[bottom-1] <= allowed {
		bottom--
	}
	if bottom-top < h/2 {
		return full
	}
	return band{top, bottom}
}

// stitchStats records what happened to every frame
type stitchStats struct {
	read       int // frames after the first
	unchanged  int // identical to the previous frame
	ambiguous  int // self-similar content, no distinctive offset
	weak       int // best offset still matched poorly
	tooSmall   int // best offset sat on the minShift floor, real scroll likely smaller
	unverified int // profiles agreed but the pixels did not
	accepted   int
	atCeiling  int // accepted at the top of the search range
	maxShift   int
	header     int // fixed rows kept once at the top
	footer     int // fixed rows kept once at the bottom
	offsets    []int
	truncated  error // stream ended in the middle of a frame
}

func (s stitchStats) report(out io.Writer, width, height int) {
	// +1 restore the first frame
	fmt.Fprintf(out, "stitched %d of %d frames -> %dx%d\n", s.accepted+1, s.read+1, width, height)
	if s.header > 0 || s.footer > 0 {
		fmt.Fprintf(out, "fixed header %d px, footer %d px\n", s.header, s.footer)
	}
	if dropped := s.read - s.accepted; dropped > 0 {
		fmt.Fprintf(out, "discarded %d: %d unchanged, %d ambiguous, %d weak, %d tiny, %d mismatch\n",
			dropped, s.unchanged, s.ambiguous, s.weak, s.tooSmall, s.unverified)
	}
	if len(s.offsets) > 0 {
		sorted := append([]int(nil), s.offsets...)
		sort.Ints(sorted)
		fmt.Fprintf(out, "shift %d..%d px, median %d\n",
			sorted[0], sorted[len(sorted)-1], sorted[len(sorted)/2])
	}
	if s.accepted == 0 {
		fmt.Fprintln(out, "! no scroll detected, output is a single frame")
	}
	if s.atCeiling > 0 {
		fmt.Fprintf(out, "! %d frame(s) hit the %d px ceiling, content may be missing\n", s.atCeiling, s.maxShift)
	}
	if s.ambiguous*4 > s.accepted {
		fmt.Fprintf(out, "! %d frame(s) too self-similar to place, expect gaps\n", s.ambiguous)
	}
	if s.truncated != nil {
		fmt.Fprintf(out, "! stream ended early: %v\n", s.truncated)
	}
}

// stitch assembles raw RGBA frames; frames that can't be placed are skipped, never guessed.
func stitch(r io.Reader, width, height int) (*image.RGBA, stitchStats, error) {
	var st stitchStats

	first, err := readFrame(r, width, height)
	if err != nil {
		return nil, st, fmt.Errorf("read first frame: %w", err)
	}

	// Hold back the first changed frames to find the fixed rows; repeats are dropped here.
	warm := []*image.RGBA{first}
	var warmErr error
	for len(warm) <= bandWarmup {
		f, err := readFrame(r, width, height)
		if err != nil {
			warmErr = err
			break
		}
		if !frameChanged(warm[len(warm)-1], f) {
			st.read++
			st.unchanged++
			continue
		}
		warm = append(warm, f)
	}
	b := detectBand(warm)
	st.header, st.footer = b.top, height-b.bottom
	st.maxShift = int(float64(b.bottom-b.top) * maxShiftFrac)

	queue := warm[1:]
	next := func() (*image.RGBA, error) {
		if len(queue) > 0 {
			f := queue[0]
			queue = queue[1:]
			return f, nil
		}
		if warmErr != nil {
			return nil, warmErr
		}
		return readFrame(r, width, height)
	}
	// Matching only sees the scrolling band; SubImage shares Pix, so the helpers index it from 0.
	content := func(f *image.RGBA) *image.RGBA {
		return f.SubImage(image.Rect(0, b.top, width, b.bottom)).(*image.RGBA)
	}

	// Rows are taken as they pass mid-band, clear of anything fading in at the band's edges.
	cut := (b.top + b.bottom) / 2
	pieces := []*image.RGBA{cropRows(first, 0, cut)}
	totalHeight := cut
	prevProfile := rowProfile(content(first))
	prevFrame := first

	for {
		curr, err := next()
		if err == io.EOF {
			break
		}
		if err != nil {
			// Keep what's stitched; likely a half-written last frame.
			st.truncated = err
			break
		}
		st.read++

		if !frameChanged(content(prevFrame), content(curr)) {
			st.unchanged++
			continue
		}

		currProfile := rowProfile(content(curr))
		offset, score, ok := findOffset(prevProfile, currProfile)
		switch {
		case !ok:
			st.ambiguous++
			continue
		case score > errThreshold:
			st.weak++
			continue
		// The search starts at minShift, so landing on it means the real scroll may be smaller.
		case offset <= minShift:
			st.tooSmall++
			continue
		}
		if !verifyOffset(content(prevFrame), content(curr), offset) {
			st.unverified++
			continue
		}

		if offset >= st.maxShift {
			st.atCeiling++
		}
		st.accepted++
		st.offsets = append(st.offsets, offset)
		// Rows about to scroll under the header exist only in prevFrame, so take them there.
		if gone := b.top + offset; gone > cut {
			pieces = append(pieces, cropRows(prevFrame, cut, gone))
			pieces = append(pieces, cropRows(curr, b.top, cut))
		} else {
			pieces = append(pieces, cropRows(curr, cut-offset, cut))
		}
		totalHeight += offset
		prevProfile = currProfile
		prevFrame = curr
	}
	// Rest of the last placed frame, footer included.
	pieces = append(pieces, cropRows(prevFrame, cut, height))
	totalHeight += height - cut

	out := image.NewRGBA(image.Rect(0, 0, width, totalHeight))
	y := 0
	for _, p := range pieces {
		ph := p.Bounds().Dy()
		draw.Draw(out, image.Rect(0, y, width, y+ph), p, image.Point{}, draw.Src)
		y += ph
	}
	return out, st, nil
}

func cropRows(img *image.RGBA, y0, y1 int) *image.RGBA {
	b := img.Bounds()
	out := image.NewRGBA(image.Rect(0, 0, b.Dx(), y1-y0))
	draw.Draw(out, out.Bounds(), img, image.Point{X: b.Min.X, Y: b.Min.Y + y0}, draw.Src)
	return out
}

func main() {
	if len(os.Args) != 4 {
		fmt.Fprintln(os.Stderr, "usage: scrollstitch <width> <height> <output.png>")
		os.Exit(1)
	}
	width, err := strconv.Atoi(os.Args[1])
	if err != nil {
		fmt.Fprintln(os.Stderr, "invalid width: ", err)
		os.Exit(1)
	}
	height, err := strconv.Atoi(os.Args[2])
	if err != nil {
		fmt.Fprintln(os.Stderr, "invalid height:", err)
		os.Exit(1)
	}
	outPath := os.Args[3]

	out, st, err := stitch(os.Stdin, width, height)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}

	f, err := os.Create(outPath)
	if err != nil {
		fmt.Fprintln(os.Stderr, "create output:", err)
		os.Exit(1)
	}
	if err := png.Encode(f, out); err != nil {
		f.Close()
		fmt.Fprintln(os.Stderr, "encode png:", err)
		os.Exit(1)
	}
	if err := f.Close(); err != nil {
		fmt.Fprintln(os.Stderr, "close output:", err)
		os.Exit(1)
	}
	st.report(os.Stderr, width, out.Bounds().Dy())
}
