package main

import (
	"bytes"
	"image"
	"math/rand"
	"testing"
)

// Every test here builds a tall master image, slides a viewport down it, and
// feeds the resulting frames back in. The master is the ground truth: a correct
// stitch reproduces it exactly, so most assertions are pixel comparisons rather
// than tolerances.

// --- masters ---------------------------------------------------------------

// randomMaster is the easy case: every row is unique, so the shift is never in doubt.
// It proves nothing subtle, but a failure here means something basic broke.
func randomMaster(w, h int, seed int64) *image.RGBA {
	m := image.NewRGBA(image.Rect(0, 0, w, h))
	rng := rand.New(rand.NewSource(seed))
	for i := 0; i < len(m.Pix); i += 4 {
		m.Pix[i] = byte(rng.Intn(256))   // R
		m.Pix[i+1] = byte(rng.Intn(256)) // G
		m.Pix[i+2] = byte(rng.Intn(256)) // B
		m.Pix[i+3] = 255                 // A
	}
	return m
}

// cardsMaster renders a list of evenly spaced cards. With unique=false every
// card is pixel-identical, which makes the vertical shift genuinely unrecoverable;
// with unique=true each card carries its own "text", the way a real feed does.
func cardsMaster(w, h int, unique bool) *image.RGBA {
	const period = 150
	m := image.NewRGBA(image.Rect(0, 0, w, h))
	for y := 0; y < h; y++ {
		card, ry := y/period, y%period
		lineLens := make([]int, 4)
		if unique {
			crng := rand.New(rand.NewSource(int64(card) * 7919))
			for i := range lineLens {
				lineLens[i] = 120 + crng.Intn(w-200)
			}
		}
		for x := 0; x < w; x++ {
			var r, g, b byte = 40, 40, 44
			if ry >= 10 && ry < 110 && x >= 20 && x < w-20 {
				r, g, b = 58, 58, 66
				if ry < 22 {
					r, g, b = 90, 96, 120
				}
				if li := (ry - 30) / 20; unique && ry >= 30 && ry < 110 && (ry-30)%20 < 8 && li < 4 {
					if x >= 40 && x < 40+lineLens[li] {
						r, g, b = 170, 172, 178
					}
				}
			}
			i := y*m.Stride + x*4
			m.Pix[i], m.Pix[i+1], m.Pix[i+2], m.Pix[i+3] = r, g, b, 255
		}
	}
	return m
}

// textureMaster gives every row the same number of dark and light pixels, so
// the per-row mean is constant down the whole image. Rows differ only in how
// finely they alternate, which is exactly what the edge profile measures.
func textureMaster(w, h int, seed int64) *image.RGBA {
	periods := []int{2, 4, 8, 10, 16, 20, 32, 40, 64, 80} // all divide 320
	m := image.NewRGBA(image.Rect(0, 0, w, h))
	rng := rand.New(rand.NewSource(seed))
	for y := 0; y < h; y++ {
		p := periods[rng.Intn(len(periods))]
		for x := 0; x < w; x++ {
			var v byte = 60
			if (x/(p/2))%2 == 1 {
				v = 200
			}
			i := y*m.Stride + x*4
			m.Pix[i], m.Pix[i+1], m.Pix[i+2], m.Pix[i+3] = v, v, v, 255
		}
	}
	return m
}

// pageMaster has flat chrome along the top and bottom with content in between,
// the shape of nearly every scrollable window. It matters because it makes the
// top of a frame resemble the bottom of the same frame, which is what lets a
// large bogus offset line up.
func pageMaster(w, h, margin int, seed int64) *image.RGBA {
	m := image.NewRGBA(image.Rect(0, 0, w, h))
	rng := rand.New(rand.NewSource(seed))
	for y := 0; y < h; y++ {
		for x := 0; x < w; x++ {
			var v byte = 42
			if y >= margin && y < h-margin {
				v = byte(50 + rng.Intn(160))
			}
			i := y*m.Stride + x*4
			m.Pix[i], m.Pix[i+1], m.Pix[i+2], m.Pix[i+3] = v, v, v, 255
		}
	}
	return m
}

// --- turning a master into a capture ---------------------------------------

// uniformTops lists the viewport positions of a steady scroll.
func uniformTops(n, step int) []int {
	tops := make([]int, n)
	for i := range tops {
		tops[i] = i * step
	}
	return tops
}

// framesFrom emits one raw RGBA frame per entry in tops, in the same byte layout
// ffmpeg pipes into the stitcher. Noise imitates the compression jitter of a real
// recording and is applied per frame, so repeating a position yields frames that
// differ slightly rather than identical ones; noise=0 when the test compares pixels.
func framesFrom(m *image.RGBA, frameH int, tops []int, noise int, seed int64) []byte {
	w := m.Bounds().Dx()
	rng := rand.New(rand.NewSource(seed))
	out := make([]byte, 0, len(tops)*w*frameH*4)
	buf := make([]byte, w*frameH*4)
	for _, top := range tops {
		for y := 0; y < frameH; y++ {
			copy(buf[y*w*4:(y+1)*w*4], m.Pix[(top+y)*m.Stride:(top+y)*m.Stride+w*4])
		}
		if noise > 0 {
			for i := 0; i < len(buf); i += 4 {
				for c := 0; c < 3; c++ {
					v := int(buf[i+c]) + rng.Intn(2*noise+1) - noise
					if v < 0 {
						v = 0
					} else if v > 255 {
						v = 255
					}
					buf[i+c] = byte(v)
				}
			}
		}
		out = append(out, buf...)
	}
	return out
}

// --- running and asserting -------------------------------------------------

func run(t *testing.T, raw []byte, w, frameH int) (*image.RGBA, stitchStats) {
	t.Helper()
	img, st, err := stitch(bytes.NewReader(raw), w, frameH)
	if err != nil {
		t.Fatalf("stitch: %v", err)
	}
	return img, st
}

// wantMaster asserts the stitch reproduced rows 0..h of the master exactly.
// It compares row by row so a failure names where the seam went wrong.
func wantMaster(t *testing.T, got, master *image.RGBA, wantH int) {
	t.Helper()
	if h := got.Bounds().Dy(); h != wantH {
		t.Fatalf("height = %d, want %d", h, wantH)
	}
	w := master.Bounds().Dx()
	for y := 0; y < wantH; y++ {
		g := got.Pix[y*got.Stride : y*got.Stride+w*4]
		m := master.Pix[y*master.Stride : y*master.Stride+w*4]
		if !bytes.Equal(g, m) {
			t.Fatalf("row %d differs from master", y)
		}
	}
}

// --- content that must stitch exactly --------------------------------------

func TestUniformScroll(t *testing.T) {
	const w, frameH, n, step = 320, 240, 40, 60
	tops := uniformTops(n, step)
	master := randomMaster(w, tops[n-1]+frameH, 1)
	got, st := run(t, framesFrom(master, frameH, tops, 0, 2), w, frameH)
	wantMaster(t, got, master, tops[n-1]+frameH)
	if st.accepted != n-1 {
		t.Errorf("accepted = %d, want %d", st.accepted, n-1)
	}
}

// Real scrolling is not steady: a wheel tick accelerates and settles, so the
// shift varies frame to frame. The largest step here sits just under the ceiling.
func TestJitteredScroll(t *testing.T) {
	const w, frameH = 320, 240
	rng := rand.New(rand.NewSource(3))
	tops := []int{0}
	for i := 0; i < 40; i++ {
		tops = append(tops, tops[len(tops)-1]+20+rng.Intn(150))
	}
	master := randomMaster(w, tops[len(tops)-1]+frameH, 4)
	got, _ := run(t, framesFrom(master, frameH, tops, 0, 5), w, frameH)
	wantMaster(t, got, master, tops[len(tops)-1]+frameH)
}

func TestRepeatingCardsWithUniqueText(t *testing.T) {
	const w, frameH, n, step = 320, 480, 30, 90
	tops := uniformTops(n, step)
	master := cardsMaster(w, tops[n-1]+frameH, true)
	got, _ := run(t, framesFrom(master, frameH, tops, 0, 8), w, frameH)
	wantMaster(t, got, master, tops[n-1]+frameH)
}

func TestRepeatingCardsSurviveNoise(t *testing.T) {
	const w, frameH, n, step = 320, 480, 30, 90
	tops := uniformTops(n, step)
	master := cardsMaster(w, tops[n-1]+frameH, true)
	got, _ := run(t, framesFrom(master, frameH, tops, 3, 9), w, frameH)
	if h := got.Bounds().Dy(); h != tops[n-1]+frameH {
		t.Errorf("height = %d, want %d", h, tops[n-1]+frameH)
	}
}

// Content whose brightness never varies can still be matched on texture alone.
// This is the only test that fails if the edge profile stops working.
func TestTextureOnlyContent(t *testing.T) {
	const w, frameH, n, step = 320, 240, 25, 60
	tops := uniformTops(n, step)
	master := textureMaster(w, tops[n-1]+frameH, 19)
	got, _ := run(t, framesFrom(master, frameH, tops, 0, 20), w, frameH)
	wantMaster(t, got, master, tops[n-1]+frameH)
}

// The counterpart to TestNoisyStillFramesAreRefused: the same page shape, this
// time actually scrolling. Without it, a matcher that refuses everything would
// still look correct.
func TestPageWithFlatMarginsScrolls(t *testing.T) {
	const w, frameH, n, step, margin = 320, 240, 25, 60, 30
	tops := uniformTops(n, step)
	master := pageMaster(w, tops[n-1]+frameH, margin, 23)
	got, _ := run(t, framesFrom(master, frameH, tops, 0, 24), w, frameH)
	wantMaster(t, got, master, tops[n-1]+frameH)
}

// --- content that must be refused ------------------------------------------

// Identical cards on a flat background make several offsets pixel-equivalent.
// The information needed to choose between them is simply not in the frames, so
// the only correct answer is to refuse: a guess here is what produces the
// duplicated bands that motivated the ratio test.
func TestIdenticalCardsAreRefused(t *testing.T) {
	const w, frameH, n, step = 320, 480, 30, 90
	tops := uniformTops(n, step)
	master := cardsMaster(w, tops[n-1]+frameH, false)
	_, st := run(t, framesFrom(master, frameH, tops, 3, 10), w, frameH)
	if st.accepted != 0 {
		t.Errorf("accepted %d frames of ambiguous content, want 0", st.accepted)
	}
}

// Frames that differ only by capture noise are not repeats, so frameChanged
// lets them through, yet they hold no new content. The search still returns a
// best offset for them, and on a page with flat chrome it lands near the top of
// the range: there the overlap is thin enough to fall entirely inside the
// margins, where the two frames genuinely do agree. Raising maxShiftFrac to 0.9
// makes the whole overlap fit inside a 30 px margin and every one of these
// frames gets accepted, inflating the result without adding a pixel of content.
func TestNoisyStillFramesAreRefused(t *testing.T) {
	const w, frameH, n, margin = 320, 240, 25, 30
	master := pageMaster(w, frameH, margin, 21)
	tops := make([]int, n) // every frame shows the same rows
	raw := framesFrom(master, frameH, tops, 2, 22)

	_, st := run(t, raw, w, frameH)
	if st.accepted != 0 {
		t.Errorf("accepted %d frames that never scrolled, want 0", st.accepted)
	}
}

// A shift larger than the search ceiling cannot be recovered. The stitch must
// drop that frame and say so, never invent a plausible-looking offset.
func TestScrollBeyondCeilingIsReported(t *testing.T) {
	const w, frameH = 320, 240
	tops := []int{0, 60, 120, 350, 410} // the 230 px jump exceeds 0.8*240
	master := randomMaster(w, tops[len(tops)-1]+frameH, 6)
	_, st := run(t, framesFrom(master, frameH, tops, 0, 7), w, frameH)
	if st.accepted == len(tops)-1 {
		t.Fatal("placed a frame that scrolled past the search ceiling")
	}
	if st.ambiguous+st.weak+st.unverified == 0 {
		t.Error("frame was dropped but no reason was recorded")
	}
}

// --- invariants ------------------------------------------------------------

// The same scroll sampled at different rates must yield the same image. This is
// the strongest check available without ground truth, and it is what exposed
// repeated frames being accepted at the search ceiling: the height grew with
// the sampling rate instead of staying put.
func TestSamplingRateDoesNotChangeResult(t *testing.T) {
	const w, frameH, maxTop, fine = 320, 300, 1600, 8
	master := randomMaster(w, maxTop+frameH, 13)

	var want *image.RGBA
	for _, every := range []int{1, 2, 4, 8} {
		var tops []int
		for top := 0; top <= maxTop; top += fine * every {
			tops = append(tops, top)
		}
		got, _ := run(t, framesFrom(master, frameH, tops, 0, 14), w, frameH)
		wantMaster(t, got, master, maxTop+frameH)
		if want == nil {
			want = got
		} else if !bytes.Equal(got.Pix, want.Pix) {
			t.Errorf("sampling every %d rows produced a different image", fine*every)
		}
	}
}

// Frames repeat whenever the recording outruns the redraw. They carry no new
// content, and accepting even one inflates the result.
func TestRepeatedFramesAreIgnored(t *testing.T) {
	const w, frameH, dup = 320, 240, 20
	tops := []int{0, 50, 100, 150, 200}
	for i := 0; i < dup; i++ {
		tops = append(tops, 200)
	}
	master := randomMaster(w, 200+frameH, 11)
	got, st := run(t, framesFrom(master, frameH, tops, 0, 12), w, frameH)
	wantMaster(t, got, master, 200+frameH)
	if st.unchanged != dup {
		t.Errorf("unchanged = %d, want %d", st.unchanged, dup)
	}
}

// --- edges -----------------------------------------------------------------

// A recording cut mid-frame should still yield everything that came before it.
func TestTruncatedStream(t *testing.T) {
	const w, frameH, n, step = 320, 240, 10, 60
	tops := uniformTops(n, step)
	master := randomMaster(w, tops[n-1]+frameH, 15)
	raw := framesFrom(master, frameH, tops, 0, 16)
	raw = append(raw, make([]byte, w*frameH*2)...) // half a frame

	got, st := run(t, raw, w, frameH)
	wantMaster(t, got, master, tops[n-1]+frameH)
	if st.truncated == nil {
		t.Error("truncation was not reported")
	}
}

func TestSingleFrame(t *testing.T) {
	const w, frameH = 320, 240
	master := randomMaster(w, frameH, 17)
	got, st := run(t, framesFrom(master, frameH, []int{0}, 0, 18), w, frameH)
	wantMaster(t, got, master, frameH)
	if st.accepted != 0 || st.read != 0 {
		t.Errorf("stats = %+v, want an empty run", st)
	}
}

func TestEmptyStreamFails(t *testing.T) {
	if _, _, err := stitch(bytes.NewReader(nil), 320, 240); err == nil {
		t.Error("expected an error for an empty stream")
	}
}
