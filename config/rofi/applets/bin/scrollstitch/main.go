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
	"os"
	"path/filepath"
	"sort"
)

const (
	minShift     = 4
	maxShiftFrac = 0.95
	errThreshold = 500.0 // max acceptable mean squared row-profile error
)

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
	b := img.Bounds()
	rgba := image.NewRGBA(b)
	draw.Draw(rgba, b, img, b.Min, draw.Src)
	return rgba, nil
}

// rowProfile returns the average luminance of each row, used as a cheap 1D
// signature for finding the vertical shift between two frames.
func rowProfile(img *image.RGBA) []float64 {
	b := img.Bounds()
	w, h := b.Dx(), b.Dy()
	profile := make([]float64, h)
	for y := 0; y < h; y++ {
		row := img.Pix[y*img.Stride : y*img.Stride+w*4]
		var sum float64
		for x := 0; x < w; x++ {
			r := float64(row[x*4])
			g := float64(row[x*4+1])
			bch := float64(row[x*4+2])
			sum += 0.299*r + 0.587*g + 0.114*bch
		}
		profile[y] = sum / float64(w)
	}
	return profile
}

// findOffset searches for the shift where prev[offset:] best matches curr[:len-offset],
// i.e. how far the content scrolled between the two frames.
func findOffset(prev, curr []float64) (offset int, score float64) {
	h := len(prev)
	maxShift := int(float64(h) * maxShiftFrac)
	bestOffset := 0
	bestErr := -1.0
	for off := minShift; off <= maxShift; off++ {
		n := h - off
		if n <= 0 {
			continue
		}
		var errSum float64
		for i := 0; i < n; i++ {
			d := prev[off+i] - curr[i]
			errSum += d * d
		}
		avgErr := errSum / float64(n)
		if bestErr < 0 || avgErr < bestErr {
			bestErr = avgErr
			bestOffset = off
		}
	}
	return bestOffset, bestErr
}

func cropRows(img *image.RGBA, y0, y1 int) *image.RGBA {
	b := img.Bounds()
	out := image.NewRGBA(image.Rect(0, 0, b.Dx(), y1-y0))
	draw.Draw(out, out.Bounds(), img, image.Point{X: b.Min.X, Y: b.Min.Y + y0}, draw.Src)
	return out
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

	for _, path := range files[1:] {
		curr, err := loadRGBA(path)
		if err != nil {
			fmt.Fprintln(os.Stderr, "skip", path, err)
			continue
		}
		if curr.Bounds().Dx() != width || curr.Bounds().Dy() != height {
			fmt.Fprintln(os.Stderr, "skip (size mismatch)", path)
			continue
		}

		currProfile := rowProfile(curr)
		offset, score := findOffset(prevProfile, currProfile)
		if score > errThreshold || offset <= minShift {
			// No reliable new content detected
			continue
		}

		pieces = append(pieces, cropRows(curr, height-offset, height))
		totalHeight += offset
		prevProfile = currProfile
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
