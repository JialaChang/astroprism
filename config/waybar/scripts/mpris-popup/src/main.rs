// Author: JialaChang

// Floating GTK popup for waybar's mpris module (click-to-open, since waybar
// has no hover-exec): shows cover art, title/artist, a seek bar, and
// prev/play-pause/next controls, driven through playerctl. Refreshes are
// triggered by D-Bus PropertiesChanged signals from playerctld rather than
// polling; only the seek bar's per-second advance is a local timer.

use std::cell::Cell;
use std::io::Read;
use std::process::{Command, Stdio};
use std::rc::Rc;

use gdk_pixbuf::Pixbuf;
use gtk::prelude::*;
use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

const PIDFILE: &str = "/tmp/waybar-mpris-popup.pid";
const ART_SIZE: i32 = 88;
const HIDE_DELAY_MS: u64 = 3000;
const POPUP_WIDTH: i32 = 420;
const POPUP_HEIGHT: i32 = 190;
/// Gap between the top of the screen and the popup (i.e. distance below the bar).
const POPUP_TOP_MARGIN: i32 = 10;
/// Approximate screen-left offset of the mpris module in modules-left.
const POPUP_LEFT_MARGIN: i32 = 230;

// playerctld proxies whichever player is currently active under one fixed bus name
const MPRIS_BUS_NAME: &str = "org.mpris.MediaPlayer2.playerctld";
const MPRIS_OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const DBUS_PROPERTIES_IFACE: &str = "org.freedesktop.DBus.Properties";

/// Signal Termination of linux
const SIGTERM: i32 = 15;

fn expand_home(path: &str) -> String {
    match path.strip_prefix("~/") {
        Some(rest) => match std::env::var("HOME") {
            Ok(home) => format!("{home}/{rest}"),
            Err(_) => path.to_string(),
        },
        None => path.to_string(),
    }
}

fn playerctl(args: &[&str]) -> String {
    Command::new("playerctl")
        .args(args)
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn already_running() -> Option<i32> {
    let content = std::fs::read_to_string(PIDFILE).ok()?;
    let pid: i32 = content.trim().parse().ok()?;
    use nix::sys::signal::kill;
    use nix::unistd::Pid;
    kill(Pid::from_raw(pid), None).ok()?;
    Some(pid)
}

fn fmt_time(seconds: f64) -> String {
    let seconds: i64 = seconds.max(0.0) as i64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

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

fn youtube_thumbnail_url(page_url: &str) -> String {
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

#[derive(Clone)]
struct Widgets {
    art: gtk::Image,
    title_label: gtk::Label,
    artist_label: gtk::Label,
    pos_label: gtk::Label,
    dur_label: gtk::Label,
    seek_scale: gtk::Scale,
    prev_btn: gtk::Button,
    playpause_btn: gtk::Button,
    next_btn: gtk::Button,
}

#[derive(Clone)]
struct Shared {
    seeking: Rc<Cell<bool>>,
    playing: Rc<Cell<bool>>,
    position: Rc<Cell<f64>>,
    length: Rc<Cell<f64>>,
}

fn set_art(widgets: &Widgets, url: &str) {
    let pixbuf = if url.is_empty() {
        None
    } else if url.starts_with("file://") {
        Pixbuf::from_file(path_from_file_url(url))
            .ok()
            .and_then(|raw| scale_cover(&raw, ART_SIZE))
    } else if url.starts_with("http://") || url.starts_with("https://") {
        fetch_url(url).and_then(|data| {
            let loader = gdk_pixbuf::PixbufLoader::new();
            loader.write(&data).ok()?;
            loader.close().ok()?;
            loader.pixbuf().and_then(|raw| scale_cover(&raw, ART_SIZE))
        })
    } else {
        None
    };

    match pixbuf {
        Some(p) => widgets.art.set_from_pixbuf(Some(&p)),
        None => widgets.art.set_from_icon_name(Some("audio-x-generic-symbolic"), gtk::IconSize::Dialog),
    }
}

fn refresh(widgets: &Widgets, shared: &Shared) {
    let status = playerctl(&["status"]);
    let has_player = !status.is_empty();
    for b in [&widgets.prev_btn, &widgets.playpause_btn, &widgets.next_btn] {
        b.set_sensitive(has_player);
    }

    if !has_player {
        widgets.title_label.set_text("No media player");
        widgets.artist_label.set_text("");
        widgets.art.set_from_icon_name(Some("audio-x-generic-symbolic"), gtk::IconSize::Dialog);
        widgets.seek_scale.set_sensitive(false);
        widgets.pos_label.set_text("0:00");
        widgets.dur_label.set_text("0:00");
        shared.playing.set(false);
        shared.position.set(0.0);
        shared.length.set(0.0);
        return;
    }

    let title = playerctl(&["metadata", "title"]);
    let title = if title.is_empty() { "Unknown title" } else { &title };
    let artist = playerctl(&["metadata", "artist"]);
    widgets
        .title_label
        .set_markup(&format!("<b>{}</b>", glib::markup_escape_text(title)));
    widgets.artist_label.set_text(&artist);

    let is_playing = status == "Playing";
    let icon = if is_playing {
        "media-playback-pause-symbolic"
    } else {
        "media-playback-start-symbolic"
    };
    widgets.playpause_btn.set_image(Some(&gtk::Image::from_icon_name(
        Some(icon),
        gtk::IconSize::LargeToolbar,
    )));

    let mut art_url = playerctl(&["metadata", "mpris:artUrl"]);
    if art_url.is_empty() {
        // Firefox doesn't expose mpris:artUrl for YouTube;
        // derive a thumbnail from the page URL instead.
        art_url = youtube_thumbnail_url(&playerctl(&["metadata", "xesam:url"]));
    }
    set_art(widgets, &art_url);

    let length_str = playerctl(&["metadata", "mpris:length"]);
    // the unit of mpris:length is μs
    let length = length_str.parse::<f64>().unwrap_or_default() / 1_000_000.0;
    let position: f64 = playerctl(&["position"]).parse().unwrap_or_default();

    shared.playing.set(is_playing);
    shared.length.set(length);
    shared.position.set(position);

    widgets.seek_scale.set_sensitive(length > 0.0);
    if !shared.seeking.get() {
        widgets.seek_scale.set_range(0.0, length.max(1.0));
        widgets.seek_scale
            .set_value(if length > 0.0 { position.min(length) } else { 0.0 });
    }
    widgets.pos_label.set_text(&fmt_time(position));
    widgets.dur_label.set_text(&fmt_time(length));
}

fn subscribe_dbus(widgets: Widgets, shared: Shared) {
    let Ok(conn) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else {
        return;
    };

    {
        let widgets = widgets.clone();
        let shared = shared.clone();
        conn.signal_subscribe(
            Some(MPRIS_BUS_NAME),
            Some(DBUS_PROPERTIES_IFACE),
            Some("PropertiesChanged"),
            Some(MPRIS_OBJECT_PATH),
            None,
            gio::DBusSignalFlags::NONE,
            move |_conn, _sender, _path, _iface, _signal, _params| {
                refresh(&widgets, &shared);
            },
        );
    }
    {
        conn.signal_subscribe(
            Some("org.freedesktop.DBus"),
            Some("org.freedesktop.DBus"),
            Some("NameOwnerChanged"),
            Some("/org/freedesktop/DBus"),
            Some(MPRIS_BUS_NAME),
            gio::DBusSignalFlags::NONE,
            move |_conn, _sender, _path, _iface, _signal, _params| {
                refresh(&widgets, &shared);
            },
        );
    }

    // Leak the connection so the subscriptions stay alive for the process's lifetime;
    // this is a short-lived, single-purpose popup, so there's nothing to clean up before exit.
    std::mem::forget(conn);
}

fn build_window() -> gtk::Window {
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Left, true);
    window.set_layer_shell_margin(Edge::Top, POPUP_TOP_MARGIN);
    window.set_layer_shell_margin(Edge::Left, POPUP_LEFT_MARGIN);
    window.set_keyboard_mode(KeyboardMode::None);

    window.set_decorated(false);
    window.set_resizable(false);
    window.set_size_request(POPUP_WIDTH, POPUP_HEIGHT);
    window.style_context().add_class("mpris-popup");

    // Needed so the CSS background's alpha channel (and the area outside
    // the rounded corners) actually renders as transparent instead of an opaque box.
    if let Some(screen) = WidgetExt::screen(&window) {
        if let Some(visual) = screen.rgba_visual() {
            window.set_visual(Some(&visual));
        }
    }

    let outer = gtk::Box::new(gtk::Orientation::Vertical, 10);
    outer.set_margin_top(14);
    outer.set_margin_bottom(14);
    outer.set_margin_start(16);
    outer.set_margin_end(16);
    window.add(&outer);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    outer.pack_start(&row, true, true, 0);

    let art = gtk::Image::new();
    art.set_size_request(ART_SIZE, ART_SIZE);
    row.pack_start(&art, false, false, 0);

    let text_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    text_box.set_valign(gtk::Align::Center);
    row.pack_start(&text_box, true, true, 0);

    let title_label = gtk::Label::new(None);
    title_label.set_xalign(0.0);
    title_label.style_context().add_class("title");
    title_label.set_line_wrap(true);
    title_label.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
    title_label.set_max_width_chars(30);
    text_box.pack_start(&title_label, false, false, 0);

    let artist_label = gtk::Label::new(None);
    artist_label.set_xalign(0.0);
    artist_label.style_context().add_class("artist");
    artist_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    artist_label.set_max_width_chars(24);
    text_box.pack_start(&artist_label, false, false, 0);

    let seek_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    outer.pack_start(&seek_row, false, false, 0);

    let pos_label = gtk::Label::new(Some("0:00"));
    pos_label.style_context().add_class("time");
    seek_row.pack_start(&pos_label, false, false, 0);

    let seek_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 1.0);
    seek_scale.set_draw_value(false);
    seek_scale.set_hexpand(true);
    seek_scale.style_context().add_class("mpris-seek");
    seek_row.pack_start(&seek_scale, true, true, 0);

    let dur_label = gtk::Label::new(Some("0:00"));
    dur_label.style_context().add_class("time");
    seek_row.pack_start(&dur_label, false, false, 0);

    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    controls.set_halign(gtk::Align::Center);
    outer.pack_start(&controls, false, false, 0);

    let make_button = |icon_name: &str| -> gtk::Button {
        let btn = gtk::Button::new();
        btn.set_image(Some(&gtk::Image::from_icon_name(
            Some(icon_name),
            gtk::IconSize::LargeToolbar,
        )));
        btn.style_context().add_class("mpris-btn");
        btn
    };
    let prev_btn = make_button("media-skip-backward-symbolic");
    let playpause_btn = make_button("media-playback-start-symbolic");
    let next_btn = make_button("media-skip-forward-symbolic");
    for b in [&prev_btn, &playpause_btn, &next_btn] {
        controls.pack_start(b, false, false, 0);
    }

    let widgets = Widgets {
        art,
        title_label,
        artist_label,
        pos_label,
        dur_label,
        seek_scale,
        prev_btn: prev_btn.clone(),
        playpause_btn: playpause_btn.clone(),
        next_btn: next_btn.clone(),
    };
    let shared = Shared {
        seeking: Rc::new(Cell::new(false)),
        playing: Rc::new(Cell::new(false)),
        position: Rc::new(Cell::new(0.0)),
        length: Rc::new(Cell::new(0.0)),
    };

    prev_btn.connect_clicked(|_| {
        playerctl(&["previous"]);
    });
    {
        let widgets = widgets.clone();
        let shared = shared.clone();
        playpause_btn.connect_clicked(move |_| {
            playerctl(&["play-pause"]);
            let widgets = widgets.clone();
            let shared = shared.clone();
            glib::source::timeout_add_local_once(std::time::Duration::from_millis(50), move || {
                refresh(&widgets, &shared);
            });
        });
    }
    next_btn.connect_clicked(|_| {
        playerctl(&["next"]);
    });

    {
        widgets.seek_scale.connect_change_value(move |_, _, value| {
            playerctl(&["position", &value.to_string()]);
            glib::Propagation::Proceed
        });
    }
    {
        let shared = shared.clone();
        widgets.seek_scale.connect_button_press_event(move |_, _| {
            shared.seeking.set(true);
            glib::Propagation::Proceed
        });
    }
    {
        let shared = shared.clone();
        widgets.seek_scale.connect_button_release_event(move |_, _| {
            shared.seeking.set(false);
            glib::Propagation::Proceed
        });
    }

    // Local per-second tick: only advances the seek bar between real D-Bus
    // events (MPRIS doesn't emit a position signal every second).
    // No playerctl/D-Bus calls happen here.
    {
        let widgets = widgets.clone();
        let shared = shared.clone();
        glib::source::timeout_add_local(std::time::Duration::from_secs(1), move || {
            if !shared.seeking.get() && shared.playing.get() && shared.length.get() > 0.0 {
                let new_pos = (shared.position.get() + 1.0).min(shared.length.get());
                shared.position.set(new_pos);
                widgets.seek_scale.set_value(new_pos);
                widgets.pos_label.set_text(&fmt_time(new_pos));
            }
            glib::ControlFlow::Continue
        });
    }

    let hide_timer: Rc<Cell<Option<glib::SourceId>>> = Rc::new(Cell::new(None));
    let start_hide_timer = {
        let hide_timer = hide_timer.clone();
        move || {
            if let Some(id) = hide_timer.take() {
                id.remove();
            }
            let id = glib::source::timeout_add_local_once(
                std::time::Duration::from_millis(HIDE_DELAY_MS),
                || gtk::main_quit(),
            );
            hide_timer.set(Some(id));
        }
    };
    let cancel_hide_timer = {
        let hide_timer = hide_timer.clone();
        move || {
            if let Some(id) = hide_timer.take() {
                id.remove();
            }
        }
    };

    {
        let cancel_hide_timer = cancel_hide_timer.clone();
        window.connect_enter_notify_event(move |_, _| {
            cancel_hide_timer();
            glib::Propagation::Proceed
        });
    }
    {
        let start_hide_timer = start_hide_timer.clone();
        window.connect_leave_notify_event(move |_, _| {
            start_hide_timer();
            glib::Propagation::Proceed
        });
    }

    refresh(&widgets, &shared);
    subscribe_dbus(widgets, shared);
    start_hide_timer();

    window
}

fn main() {
    if let Some(pid) = already_running() {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;
        let _ = kill(Pid::from_raw(pid), Signal::SIGTERM);
        return;
    }

    gtk::init().expect("failed to initialize GTK");

    let css_path = expand_home("~/.config/waybar/mpris-popup.css");
    let provider = gtk::CssProvider::new();
    if provider.load_from_path(&css_path).is_ok() {
        if let Some(screen) = gdk::Screen::default() {
            gtk::StyleContext::add_provider_for_screen(
                &screen,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    }

    let _ = std::fs::write(PIDFILE, std::process::id().to_string());

    glib::source::unix_signal_add(SIGTERM, || {
        gtk::main_quit();
        glib::ControlFlow::Break
    });

    let window = build_window();
    window.show_all();
    gtk::main();

    let _ = std::fs::remove_file(PIDFILE);
}
