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
	"io"
	"math"
	"os"
	"strconv"
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

	first, err := readFrame(os.Stdin, width, height)
	if err != nil {
		fmt.Fprintln(os.Stderr, "read first frame: ", err)
		os.Exit(1)
	}

	pieces := []*image.RGBA{first}
	totalHeight := height
	prevProfile := rowProfile(first)
	prevFrame := first

	for {
		curr, err := readFrame(os.Stdin, width, height)
		if err == io.EOF || err == io.ErrUnexpectedEOF {
			break
		}
		if err != nil {
			fmt.Fprintln(os.Stderr, "read frame: ", err)
			break
		}

		currProfile := rowProfile(curr)
		offset, score := findOffset(prevProfile, currProfile)
		if score > errThreshold || offset <= minShift {
			// No reliable new content detected
			continue
		}
		if !verifyOffset(prevFrame, curr, offset) {
			continue
		}

		pieces = append(pieces, cropRows(curr, height-offset, height))
		totalHeight += offset
		prevProfile = currProfile
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
