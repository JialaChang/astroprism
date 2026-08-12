// Author: JialaChang

// Floating GTK popup for waybar's mpris module (click-to-open, since waybar
// has no hover-exec): shows cover art, title/artist, a seek bar, and
// prev/play-pause/next controls, driven through playerctl. Refreshes are
// triggered by D-Bus PropertiesChanged signals from playerctld rather than
// polling; only the seek bar's per-second advance is a local timer.

use std::cell::{Cell, RefCell};
use std::io::Read;
use std::process::{Command, Stdio};
use std::rc::Rc;

use gdk_pixbuf::Pixbuf;
use gtk::prelude::*;
use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

const PIDFILE: &str = "/tmp/waybar-mpris-popup.pid";
const ART_SIZE: i32 = 84;
/// Corner rounding of the cover art, in pixels.
const ART_RADIUS: f64 = 12.0;
const HIDE_DELAY_MS: u64 = 3000;
const POPUP_WIDTH: i32 = 380;
const POPUP_HEIGHT: i32 = 120;
/// Gap between the top of the screen and the popup (i.e. distance below the bar).
const POPUP_TOP_MARGIN: i32 = 10;
/// Approximate screen-left offset of the mpris module in modules-left.
const POPUP_LEFT_MARGIN: i32 = 130;

// playerctld proxies whichever player is currently active under one fixed bus name
const MPRIS_BUS_NAME: &str = "org.mpris.MediaPlayer2.playerctld";
const MPRIS_OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const DBUS_PROPERTIES_IFACE: &str = "org.freedesktop.DBus.Properties";
const MPRIS_PLAYER_IFACE: &str = "org.mpris.MediaPlayer2.Player";
const MPRIS_BUS_PREFIX: &str = "org.mpris.MediaPlayer2.";

/// Wayland layer-surface namespace; what compositor layer rules match on.
const LAYER_NAMESPACE: &str = "mpris-popup";

/// Signal Termination of linux
const SIGTERM: i32 = 15;

/// Everything a refresh needs, in one playerctl call instead of one per field —
/// each spawn costs ~10ms on the UI thread. Fields are separated by an ASCII
/// unit separator, which playerctl passes through and metadata never contains.
const METADATA_FORMAT: &str = concat!(
    "{{playerName}}\u{1f}",
    "{{title}}\u{1f}",
    "{{artist}}\u{1f}",
    "{{mpris:artUrl}}\u{1f}",
    "{{mpris:length}}\u{1f}",
    "{{position}}\u{1f}",
    "{{xesam:url}}",
);
const METADATA_SEP: char = '\u{1f}';

/// Cache sentinel for "no cover on screen"; never equal to a real art URL,
/// including the empty one, so the next refresh always tries again.
const ART_URL_NONE: &str = "\u{0}";

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

/// PID of the process behind a player, by asking the bus who owns its name.
fn player_pid(player_name: &str) -> Option<u32> {
    let conn = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok()?;
    let call = |method: &str, args: Option<&glib::Variant>| {
        conn.call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            method,
            args,
            None,
            gio::DBusCallFlags::NONE,
            1000,
            gio::Cancellable::NONE,
        )
        .ok()
    };

    let names: Vec<String> = call("ListNames", None)?.child_value(0).get()?;
    // playerctl reports the base name ("firefox") while the bus name carries an
    // instance suffix ("…firefox.instance_1_96"), so match on both forms.
    let wanted = format!("{MPRIS_BUS_PREFIX}{player_name}");
    let instance_prefix = format!("{wanted}.");
    let bus_name = names
        .into_iter()
        .find(|n| *n == wanted || n.starts_with(&instance_prefix))?;

    call("GetConnectionUnixProcessID", Some(&(bus_name,).to_variant()))?
        .child_value(0)
        .get()
}

/// Focus the player's own window. MPRIS has a Raise method for this, but on
/// Wayland a client cannot raise itself without an activation token and it does
/// nothing here, so go through the compositor. Hyprland's Lua config mode also
/// rejects the old `hyprctl dispatch focuswindow` syntax, hence `eval`.
fn focus_player_window(player_name: &str) -> bool {
    let Some(pid) = player_pid(player_name) else {
        return false;
    };
    Command::new("hyprctl")
        .args([
            "eval",
            &format!("hl.dispatch(hl.dsp.focus({{ window = \"pid:{pid}\" }}))"),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

/// Nerd Font glyph for a player, kept in sync with `mpris.player-icons` in config.jsonc.
fn player_icon(player_name: &str) -> &'static str {
    let name = player_name.to_ascii_lowercase();
    if name.starts_with("spotify") {
        "\u{f1bc}"
    } else if name.starts_with("firefox") {
        "\u{e745}"
    } else {
        "\u{f144}"
    }
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
    window: gtk::Window,
    art: gtk::Image,
    status_label: gtk::Label,
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
    /// Art URL the popup is currently showing or fetching, so refreshes
    /// triggered by play/pause or a position change don't re-fetch the same
    /// cover, and so a late download can tell whether it is still wanted.
    art_url: Rc<RefCell<String>>,
    /// Downloaded cover bytes on their way back to the main thread.
    art_tx: glib::Sender<(String, Vec<u8>)>,
    /// Player behind the current track, for the click-to-focus label.
    player_name: Rc<RefCell<String>>,
}

fn set_art(widgets: &Widgets, shared: &Shared, url: &str) {
    if *shared.art_url.borrow() == url {
        return;
    }
    shared.art_url.replace(url.to_string());

    if url.starts_with("http://") || url.starts_with("https://") {
        // Downloads block for ~120ms, which on the main thread would freeze the
        // whole popup on every track change. Fetch on a worker thread and paint
        // when the bytes come back; the old cover stays up in the meantime.
        let tx = shared.art_tx.clone();
        let url = url.to_string();
        std::thread::spawn(move || {
            let data = fetch_url(&url).unwrap_or_default();
            let _ = tx.send((url, data));
        });
        return;
    }

    // Local files decode in a millisecond or two, so they stay inline.
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
fn apply_art(widgets: &Widgets, shared: &Shared, url: &str, pixbuf: Option<Pixbuf>) {
    match pixbuf.and_then(|p| round_pixbuf(&p, ART_RADIUS).or(Some(p))) {
        Some(p) => widgets.art.set_from_pixbuf(Some(&p)),
        None => {
            widgets.art.set_from_icon_name(Some("audio-x-generic-symbolic"), gtk::IconSize::Dialog);
            // Only cache the miss when there was nothing to load in the first
            // place, so a failed download is retried on the next refresh.
            if !url.is_empty() {
                shared.art_url.replace(ART_URL_NONE.to_string());
            }
        }
    }
}

/// Set the pointer's shape over the popup; `None` restores the inherited one.
fn set_cursor(window: &gtk::Window, name: Option<&str>) {
    let Some(gdk_window) = WidgetExt::window(window) else {
        return;
    };
    let cursor = name.and_then(|name| {
        gdk::Display::default().and_then(|display| gdk::Cursor::from_name(&display, name))
    });
    gdk_window.set_cursor(cursor.as_ref());
}

/// Whether a leave event means the pointer really left the popup. GTK also
/// sends leave events to the window when the pointer merely moves from the
/// window onto one of its own children — a button, the seek bar — and those
/// arrive with detail Inferior. Treating them as "left the popup" is what made
/// the popup close while the pointer was resting on a control. Everything else
/// counts as an exit: erring towards closing is safer than a popup that stays
/// up forever, and any enter event cancels the timer again.
fn is_pointer_exit(ev: &gdk::EventCrossing) -> bool {
    ev.detail() != gdk::NotifyType::Inferior
}

fn decode_art(data: &[u8]) -> Option<Pixbuf> {
    let loader = gdk_pixbuf::PixbufLoader::new();
    loader.write(data).ok()?;
    loader.close().ok()?;
    loader.pixbuf().and_then(|raw| scale_cover(&raw, ART_SIZE))
}

/// "{player icon}  {PLAYER NAME}", mirroring how the bar module labels itself.
/// Also records the player so clicking the label can find its window.
fn set_status_label(widgets: &Widgets, shared: &Shared, player_name: &str) {
    shared.player_name.replace(player_name.to_string());
    // playerctl reports firefox as e.g. "firefox.instance_1_96"
    let short_name = player_name.split('.').next().unwrap_or(player_name);
    let label = if short_name.is_empty() {
        "NO PLAYER".to_string()
    } else {
        short_name.to_uppercase()
    };
    widgets.status_label.set_markup(&format!(
        "{}  <span letter_spacing=\"900\">{}</span>",
        player_icon(player_name),
        glib::markup_escape_text(&label),
    ));
}

fn set_playpause_icon(widgets: &Widgets, is_playing: bool) {
    let icon = if is_playing {
        "media-playback-pause-symbolic"
    } else {
        "media-playback-start-symbolic"
    };
    widgets.playpause_btn.set_image(Some(&gtk::Image::from_icon_name(
        Some(icon),
        gtk::IconSize::LargeToolbar,
    )));
}

/// Mirrors `#mpris.paused` in the bar's stylesheet: the player label, the seek
/// bar and the play button all drop back to neutral colours while paused.
fn set_paused_class(widgets: &Widgets, paused: bool) {
    let ctx = widgets.window.style_context();
    if paused {
        ctx.add_class("paused");
    } else {
        ctx.remove_class("paused");
    }
}

fn refresh(widgets: &Widgets, shared: &Shared) {
    let status = playerctl(&["status"]);
    let has_player = !status.is_empty();
    for b in [&widgets.prev_btn, &widgets.playpause_btn, &widgets.next_btn] {
        b.set_sensitive(has_player);
    }

    if !has_player {
        set_status_label(widgets, shared, "");
        set_paused_class(widgets, true);
        set_playpause_icon(widgets, false);
        widgets.title_label.set_text("No media player");
        widgets.artist_label.set_text("");
        set_art(widgets, shared, "");
        widgets.seek_scale.set_sensitive(false);
        widgets.pos_label.set_text("0:00");
        widgets.dur_label.set_text("0:00");
        shared.playing.set(false);
        shared.position.set(0.0);
        shared.length.set(0.0);
        return;
    }

    let metadata = playerctl(&["metadata", "--format", METADATA_FORMAT]);
    let mut fields = metadata.split(METADATA_SEP);
    let mut next_field = move || fields.next().unwrap_or_default();
    let (player_name, title, artist) = (next_field(), next_field(), next_field());
    let (art_url, length_str, position_str, page_url) =
        (next_field(), next_field(), next_field(), next_field());

    set_status_label(widgets, shared, player_name);

    let title = if title.is_empty() { "Unknown title" } else { title };
    widgets
        .title_label
        .set_markup(&format!("<b>{}</b>", glib::markup_escape_text(title)));
    widgets.artist_label.set_text(artist);

    let is_playing = status == "Playing";
    set_paused_class(widgets, !is_playing);
    set_playpause_icon(widgets, is_playing);

    let art_url = if art_url.is_empty() {
        // Firefox doesn't expose mpris:artUrl for YouTube;
        // derive a thumbnail from the page URL instead.
        youtube_thumbnail_url(page_url)
    } else {
        art_url.to_string()
    };
    set_art(widgets, shared, &art_url);

    // the unit of mpris:length and {{position}} is μs
    let length = length_str.parse::<f64>().unwrap_or_default() / 1_000_000.0;
    let position = position_str.parse::<f64>().unwrap_or_default() / 1_000_000.0;

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
        // MPRIS marks Position as non-notifying: seeking emits Seeked on the
        // player interface and no PropertiesChanged at all, so without this the
        // popup never learns about a seek — including its own.
        let widgets = widgets.clone();
        let shared = shared.clone();
        conn.signal_subscribe(
            Some(MPRIS_BUS_NAME),
            Some(MPRIS_PLAYER_IFACE),
            Some("Seeked"),
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
    // Own namespace instead of the default "gtk-layer-shell", so compositor
    // rules (animations, blur) can target this popup and nothing else.
    window.set_namespace(LAYER_NAMESPACE);
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
    outer.set_margin_bottom(12);
    outer.set_margin_start(16);
    outer.set_margin_end(16);
    window.add(&outer);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    outer.pack_start(&row, true, true, 0);

    let art = gtk::Image::new();
    art.set_size_request(ART_SIZE, ART_SIZE);
    art.set_valign(gtk::Align::Center);
    row.pack_start(&art, false, false, 0);

    let text_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text_box.set_valign(gtk::Align::Center);
    row.pack_start(&text_box, true, true, 0);

    let status_label = gtk::Label::new(None);
    status_label.set_xalign(0.0);
    status_label.style_context().add_class("status");
    // Ellipsizing makes the label's minimum width one ellipsis wide, which the
    // button around it would then happily shrink to; keep a floor under it.
    status_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    status_label.set_width_chars(14);

    // A button rather than a bare label, so hover states and clicks come for
    // free; the CSS strips the button chrome so it still reads as a label.
    let player_btn = gtk::Button::new();
    player_btn.set_relief(gtk::ReliefStyle::None);
    player_btn.set_halign(gtk::Align::Start);
    player_btn.set_margin_bottom(2);
    player_btn.style_context().add_class("player-link");
    player_btn.add(&status_label);
    text_box.pack_start(&player_btn, false, false, 0);

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

    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    controls.set_halign(gtk::Align::Center);
    outer.pack_start(&controls, false, false, 0);

    let make_button = |icon_name: &str, size: gtk::IconSize| -> gtk::Button {
        let btn = gtk::Button::new();
        btn.set_image(Some(&gtk::Image::from_icon_name(Some(icon_name), size)));
        btn.set_relief(gtk::ReliefStyle::None);
        btn.style_context().add_class("mpris-btn");
        btn
    };
    // Skip buttons sit a size below play/pause so the accent button leads.
    let prev_btn = make_button("media-skip-backward-symbolic", gtk::IconSize::Button);
    let playpause_btn = make_button("media-playback-start-symbolic", gtk::IconSize::LargeToolbar);
    // The accent-filled button, styled after the mpris module in the bar.
    playpause_btn.style_context().add_class("play");
    let next_btn = make_button("media-skip-forward-symbolic", gtk::IconSize::Button);
    for b in [&prev_btn, &playpause_btn, &next_btn] {
        controls.pack_start(b, false, false, 0);
    }

    let widgets = Widgets {
        window: window.clone(),
        art,
        status_label,
        title_label,
        artist_label,
        pos_label,
        dur_label,
        seek_scale,
        prev_btn: prev_btn.clone(),
        playpause_btn: playpause_btn.clone(),
        next_btn: next_btn.clone(),
    };
    // Deprecated in favour of async channels, which would mean pulling in
    // async-channel and a futures executor for one message type; revisit when
    // this moves to glib 0.19+, where the old API is gone.
    #[allow(deprecated)]
    let (art_tx, art_rx) = glib::MainContext::channel::<(String, Vec<u8>)>(glib::Priority::DEFAULT);
    let shared = Shared {
        seeking: Rc::new(Cell::new(false)),
        playing: Rc::new(Cell::new(false)),
        position: Rc::new(Cell::new(0.0)),
        length: Rc::new(Cell::new(0.0)),
        art_url: Rc::new(RefCell::new(ART_URL_NONE.to_string())),
        art_tx,
        player_name: Rc::new(RefCell::new(String::new())),
    };

    {
        let shared = shared.clone();
        player_btn.connect_clicked(move |_| {
            let player_name = shared.player_name.borrow().clone();
            if !player_name.is_empty() && focus_player_window(&player_name) {
                // The popup has done its job once you're looking at the player.
                gtk::main_quit();
            }
        });
    }
    // Hand cursor over the label, the only hint that it is clickable.
    {
        let window = window.clone();
        player_btn.connect_enter_notify_event(move |_, _| {
            set_cursor(&window, Some("pointer"));
            glib::Propagation::Proceed
        });
    }
    {
        let window = window.clone();
        player_btn.connect_leave_notify_event(move |_, _| {
            set_cursor(&window, None);
            glib::Propagation::Proceed
        });
    }

    {
        let widgets = widgets.clone();
        let shared = shared.clone();
        art_rx.attach(None, move |(url, data)| {
            // Skipping through tracks can leave several downloads in flight;
            // only the one still wanted gets painted, whatever order they land in.
            if *shared.art_url.borrow() == url {
                apply_art(&widgets, &shared, &url, decode_art(&data));
            }
            glib::ControlFlow::Continue
        });
    }

    prev_btn.connect_clicked(|_| {
        playerctl(&["previous"]);
    });
    {
        let widgets = widgets.clone();
        let shared = shared.clone();
        playpause_btn.connect_clicked(move |_| {
            // Flip the button before asking the player to, so the click feels
            // instant; the D-Bus signal that follows confirms the real state,
            // and the timer below covers players that never emit one.
            let is_playing = !shared.playing.get();
            shared.playing.set(is_playing);
            set_playpause_icon(&widgets, is_playing);
            set_paused_class(&widgets, !is_playing);

            playerctl(&["play-pause"]);

            let widgets = widgets.clone();
            let shared = shared.clone();
            glib::source::timeout_add_local_once(std::time::Duration::from_millis(200), move || {
                refresh(&widgets, &shared);
            });
        });
    }
    next_btn.connect_clicked(|_| {
        playerctl(&["next"]);
    });

    {
        let widgets_seek = widgets.clone();
        let shared_seek = shared.clone();
        widgets.seek_scale.connect_change_value(move |_, _, value| {
            // Keep the local model in step with the drag. Without this the
            // per-second tick keeps counting on from the pre-seek position and
            // drags the handle back to where it started.
            // Players that report no length leave the range open-ended.
            let length = shared_seek.length.get();
            let value = if length > 0.0 { value.clamp(0.0, length) } else { value.max(0.0) };
            shared_seek.position.set(value);
            widgets_seek.pos_label.set_text(&fmt_time(value));

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
        window.connect_leave_notify_event(move |_, ev| {
            if is_pointer_exit(ev) {
                start_hide_timer();
            }
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
