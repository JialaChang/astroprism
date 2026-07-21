// Author: JialaChang

// scrollstitch stitches a sequence of screen-capture frames (extracted from a
// scrolling recording) into a single tall PNG by detecting the vertical
// overlap between consecutive frames.

package main

import (
	"fmt"
	"image"
	"image/draw"
	"image/png"
	"math"
	"os"
	"path/filepath"
	"runtime"
	"sort"
)

const (
	minShift     = 4
	maxShiftFrac = 0.8
	errThreshold = 500.0 // max acceptable mean squared row-profile error
)

type frameProfile struct {
	mean []float64 // per-row average luma
	edge []float64 // per-row horizontal total variation (L1), a robust edge-density proxy
}

type frameResult struct {
	path    string
	img     *image.RGBA
	profile frameProfile
	err     error
}

func loadRGBA(path string) (*image.RGBA, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()

	img, err := png.Decode(f)
	if err != nil {
		return nil, err
	}
	// ffmpeg's truecolor PNG already decode into *image.RGBA
	if rgba, ok := img.(*image.RGBA); ok {
		return rgba, nil
	}
	// Fallback for any other pixel format
	b := img.Bounds()
	rgba := image.NewRGBA(b)
	draw.Draw(rgba, b, img, b.Min, draw.Src)
	return rgba, nil
}

// rowProfile computes two per-row 1D signatures used to find the vertical shift between frames:
// the average Rec. 709 luminance, and the horizontal total variation
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
				if d < 0 {
					d = -d
				}
				edgeSum += d
			}
			prevLuma = luma
		}
		profile.mean[y] = float64(lumaSum) / float64(w*256)
		// x==0 has no left neighbor, so edgeSum holds w-1 samples
		profile.edge[y] = float64(edgeSum) / float64((w-1)*256)
	}
	return profile
}

// findOffset searches for the shift where prev[offset:] best matches curr[:len-offset],
// i.e. how far the content scrolled between the two frames.
func findOffset(prev, curr frameProfile) (offset int, score float64) {
	h := len(prev.mean)
	maxShift := int(float64(h) * maxShiftFrac)
	bestOffset := 0
	bestErr := math.MaxFloat64
	for offset := minShift; offset <= maxShift; offset++ {
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

const (
	verifyRowStep   = 8
	verifyColStep   = 16
	verifyThreshold = 12.0 // max mean abs green diff in the overlap region
)

// verifyOffset double-checks a candidate offset against real pixels:
// 1D profiles can collide on self-similar content, but actual
// 2D pixels at a wrong offset almost never agree.
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
			if d < 0 {
				d = -d
			}
			sum += d
			count++
		}
	}
	if count == 0 {
		return false
	}
	return float64(sum)/float64(count) <= verifyThreshold
}

func cropRows(img *image.RGBA, y0, y1 int) *image.RGBA {
	b := img.Bounds()
	out := image.NewRGBA(image.Rect(0, 0, b.Dx(), y1-y0))
	draw.Draw(out, out.Bounds(), img, image.Point{X: b.Min.X, Y: b.Min.Y + y0}, draw.Src)
	return out
}

// decodes frames and computes their profiles in parallel across CPUs
// while delivering results strictly in file order.
// At most `window` decoded frames are held in memory waiting to be consumed.
func decodeFrame(files []string, windows int) <-chan chan frameResult {
	queue := make(chan chan frameResult, windows)
	go func() {
		semaphore := make(chan struct{}, runtime.NumCPU())
		for _, path := range files {
			ch := make(chan frameResult, 1)
			queue <- ch // block when the window is full, bounding memory
			semaphore <- struct{}{}
			go func(path string) {
				defer func() { <-semaphore }()
				img, err := loadRGBA(path)
				var profile frameProfile
				if err == nil {
					profile = rowProfile(img)
				}
				ch <- frameResult{path, img, profile, err}
			}(path)
		}
		close(queue)
	}()
	return queue
}

func main() {
	if len(os.Args) != 3 {
		fmt.Fprintln(os.Stderr, "usage: scrollstitch <frames_dir> <output.png>")
		os.Exit(1)
	}
	framesDir, outPath := os.Args[1], os.Args[2]

	entries, err := os.ReadDir(framesDir)
	if err != nil {
		fmt.Fprintln(os.Stderr, "read frames dir:", err)
		os.Exit(1)
	}
	var files []string
	for _, e := range entries {
		if !e.IsDir() && filepath.Ext(e.Name()) == ".png" {
			files = append(files, filepath.Join(framesDir, e.Name()))
		}
	}
	sort.Strings(files)
	if len(files) == 0 {
		fmt.Fprintln(os.Stderr, "no frames found in", framesDir)
		os.Exit(1)
	}

	first, err := loadRGBA(files[0])
	if err != nil {
		fmt.Fprintln(os.Stderr, "load", files[0], err)
		os.Exit(1)
	}
	width := first.Bounds().Dx()
	height := first.Bounds().Dy()

	pieces := []*image.RGBA{first}
	totalHeight := height
	prevProfile := rowProfile(first)
	prevFrame := first

	for channel := range decodeFrame(files[1:], 16) {
		result := <-channel
		if result.err != nil {
			fmt.Fprintln(os.Stderr, "skip:", result.path, result.err)
			continue
		}
		curr := result.img
		if curr.Bounds().Dx() != width || curr.Bounds().Dy() != height {
			fmt.Fprintln(os.Stderr, "skip (size mismatch)", result.path)
			continue
		}

		offset, score := findOffset(prevProfile, result.profile)
		if score > errThreshold || offset <= minShift {
			// No reliable new content detected
			continue
		}
		if !verifyOffset(prevFrame, curr, offset) {
			continue
		}

		pieces = append(pieces, cropRows(curr, height-offset, height))
		totalHeight += offset
		prevProfile = result.profile
		prevFrame = curr
	}

	if len(pieces) == 1 {
		fmt.Fprintln(os.Stderr, "warning: no scroll detected, output is a single frame")
	}

	out := image.NewRGBA(image.Rect(0, 0, width, totalHeight))
	y := 0
	for _, p := range pieces {
		ph := p.Bounds().Dy()
		draw.Draw(out, image.Rect(0, y, width, y+ph), p, image.Point{}, draw.Src)
		y += ph
	}

	f, err := os.Create(outPath)
	if err != nil {
		fmt.Fprintln(os.Stderr, "create output:", err)
		os.Exit(1)
	}
	defer f.Close()
	if err := png.Encode(f, out); err != nil {
		fmt.Fprintln(os.Stderr, "encode png:", err)
		os.Exit(1)
	}
	fmt.Printf("stitched %d frame(s) -> %dx%d\n", len(pieces), width, totalHeight)
}
