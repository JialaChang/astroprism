// Cover art: fetching, decoding, scaling and rounding.

use std::io::Read;

use gdk_pixbuf::Pixbuf;
use gtk::prelude::*;

use crate::ui::{Shared, Widgets};

pub(crate) const ART_SIZE: i32 = 84;
/// Corner rounding of the cover art, in pixels
const ART_RADIUS: f64 = 12.0;

/// Cache sentinel for "no cover on screen"; never equal to a real art URL,
/// including the empty one, so the next refresh always tries again.
pub(crate) const ART_URL_NONE: &str = "\u{0}";

/// Scale+crop to fill a size x size square without distorting aspect ratio.
fn scale_cover(pixbuf: &Pixbuf, size: i32) -> Option<Pixbuf> {
    let (w, h) = (pixbuf.width(), pixbuf.height());
    if w == 0 || h == 0 {
        return None;
    }
    let scale = (size as f64 / w as f64).max(size as f64 / h as f64);
    let new_w = ((w as f64 * scale).round() as i32).max(size);
    let new_h = ((h as f64 * scale).round() as i32).max(size);
    let scaled = pixbuf.scale_simple(new_w, new_h, gdk_pixbuf::InterpType::Bilinear)?;
    let x = (new_w - size) / 2;
    let y = (new_h - size) / 2;
    Some(scaled.new_subpixbuf(x, y, size, size))
}

/// Round off the cover art's corners. GTK3 can't clip a GtkImage's pixbuf with
/// CSS, so the rounding has to be painted into the pixbuf itself.
fn round_pixbuf(pixbuf: &Pixbuf, radius: f64) -> Option<Pixbuf> {
    use gdk::cairo::{Context, Format, ImageSurface};
    use gdk::prelude::GdkContextExt;
    use std::f64::consts::{FRAC_PI_2, PI};

    let (w, h) = (pixbuf.width(), pixbuf.height());
    let surface = ImageSurface::create(Format::ARgb32, w, h).ok()?;
    let cr = Context::new(&surface).ok()?;
    let (w, h) = (w as f64, h as f64);
    let r = radius.min(w / 2.0).min(h / 2.0);

    cr.new_sub_path();
    cr.arc(w - r, r, r, -FRAC_PI_2, 0.0);
    cr.arc(w - r, h - r, r, 0.0, FRAC_PI_2);
    cr.arc(r, h - r, r, FRAC_PI_2, PI);
    cr.arc(r, r, r, PI, PI + FRAC_PI_2);
    cr.close_path();
    cr.clip();

    cr.set_source_pixbuf(pixbuf, 0.0, 0.0);
    cr.paint().ok()?;
    drop(cr);

    gdk::pixbuf_get_from_surface(&surface, 0, 0, w as i32, h as i32)
}

pub(crate) fn youtube_thumbnail_url(page_url: &str) -> String {
    if page_url.is_empty() {
        return String::new();
    }
    let re = regex::Regex::new(r"(?:v=|youtu\.be/|embed/|shorts/)([A-Za-z0-9_-]{11})").unwrap();
    match re.captures(page_url) {
        Some(caps) => format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", &caps[1]),
        None => String::new(),
    }
}

fn fetch_url(url: &str) -> Option<Vec<u8>> {
    let mut resp = ureq::get(url)
        .header("User-Agent", "Mozilla/5.0")
        .call()
        .ok()?;
    let mut data = Vec::new();
    resp.body_mut().as_reader().read_to_end(&mut data).ok()?;
    Some(data)
}

fn path_from_file_url(url: &str) -> String {
    let raw = url.strip_prefix("file://").unwrap_or(url);
    // Minimal percent-decoding; local mpris art paths rarely need more.
    let mut out = Vec::new();
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&raw[i + 1..i + 3], 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub(crate) fn set_art(widgets: &Widgets, shared: &Shared, url: &str) {
    if *shared.art_url.borrow() == url {
        return;
    }
    shared.art_url.replace(url.to_string());

    if url.starts_with("http://") || url.starts_with("https://") {
        // Fetch remote covers on a thread and paint them when the bytes come back;
        // the old cover stays up in the meantime.
        let tx = shared.art_tx.clone();
        let url = url.to_string();
        std::thread::spawn(move || {
            let data = fetch_url(&url).unwrap_or_default();
            let _ = tx.send((url, data));
        });
        return;
    }

    // Local files decode fast enough to stay on the main thread.
    let pixbuf = if url.starts_with("file://") {
        Pixbuf::from_file(path_from_file_url(url))
            .ok()
            .and_then(|raw| scale_cover(&raw, ART_SIZE))
    } else {
        None
    };
    apply_art(widgets, shared, url, pixbuf);
}

/// Paint a cover that has finished loading, or fall back to the placeholder.
pub(crate) fn apply_art(widgets: &Widgets, shared: &Shared, url: &str, pixbuf: Option<Pixbuf>) {
    match pixbuf.and_then(|p| round_pixbuf(&p, ART_RADIUS).or(Some(p))) {
        Some(p) => widgets.art.set_from_pixbuf(Some(&p)),
        None => {
            widgets.art.set_from_icon_name(Some("audio-x-generic-symbolic"), gtk::IconSize::Dialog);
            // Only cache the miss when there was nothing to load in the first place,
            // so a failed download is retried on the next refresh.
            if !url.is_empty() {
                shared.art_url.replace(ART_URL_NONE.to_string());
            }
        }
    }
}

pub(crate) fn decode_art(data: &[u8]) -> Option<Pixbuf> {
    let loader = gdk_pixbuf::PixbufLoader::new();
    loader.write(data).ok()?;
    loader.close().ok()?;
    loader.pixbuf().and_then(|raw| scale_cover(&raw, ART_SIZE))
}
