// The popup window: widget tree, D-Bus subscriptions, and the refresh that
// keeps them in sync with the active player.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::art;
use crate::mpris;

const HIDE_DELAY_MS: u64 = 3000;
/// How long the drag has to settle before the seek is actually sent.
const SEEK_DEBOUNCE_MS: u64 = 80;
/// How long to let a burst of D-Bus signals settle before refreshing.
const REFRESH_COALESCE_MS: u64 = 30;

const POPUP_WIDTH: i32 = 380;
const POPUP_HEIGHT: i32 = 120;
/// Gap between the top of the screen and the popup
const POPUP_TOP_MARGIN: i32 = 10;
/// Fallback screen-left offset, used when the host file below is missing
const POPUP_LEFT_MARGIN_DEFAULT: i32 = 275;
/// Per-machine screen-left offset of the mpris module:
/// deploy.sh writes this from config/hosts/<profile>/mpris-popup-margin
const POPUP_LEFT_MARGIN_PATH: &str = "~/.config/waybar/mpris-popup-margin";

/// Wayland layer-surface namespace; what compositor layer rules match on
const LAYER_NAMESPACE: &str = "mpris-popup";

/// Read the per-host left margin, falling back to the default if the file is
/// missing or unparseable.
fn popup_left_margin() -> i32 {
    std::fs::read_to_string(crate::expand_home(POPUP_LEFT_MARGIN_PATH))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(POPUP_LEFT_MARGIN_DEFAULT)
}

/// Send the position the drag last landed on, cancelling any scheduled send.
fn send_seek(shared: &Shared) {
    if let Some(id) = shared.pending_seek.take() {
        id.remove();
    }
    mpris::playerctl(&["position", &shared.position.get().to_string()]);
}

fn fmt_time(seconds: f64) -> String {
    let seconds: i64 = seconds.max(0.0) as i64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

#[derive(Clone)]
pub(crate) struct Widgets {
    window: gtk::Window,
    pub(crate) art: gtk::Image,
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
pub(crate) struct Shared {
    seeking: Rc<Cell<bool>>,
    /// Debounces seeks: playerctl blocks the main loop, change-value fires per frame.
    pending_seek: Rc<Cell<Option<glib::SourceId>>>,
    /// Set while a refresh is scheduled, so a burst of signals costs one refresh.
    refresh_queued: Rc<Cell<bool>>,
    playing: Rc<Cell<bool>>,
    position: Rc<Cell<f64>>,
    length: Rc<Cell<f64>>,
    /// Art URL the popup is currently showing or fetching, so a refresh doesn't
    /// re-fetch the same cover and a late download can tell if it is still wanted.
    pub(crate) art_url: Rc<RefCell<String>>,
    /// Downloaded cover bytes on their way back to the main thread.
    pub(crate) art_tx: glib::Sender<(String, Vec<u8>)>,
    /// Player behind the current track, for the click-to-focus label.
    player_name: Rc<RefCell<String>>,
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

/// Whether a leave event means the pointer really left the popup, rather than
/// just moving onto one of its own children.
fn is_pointer_exit(ev: &gdk::EventCrossing) -> bool {
    ev.detail() != gdk::NotifyType::Inferior
}

/// "{player icon}  {PLAYER NAME}", mirroring how the bar module labels itself.
/// Also records the player so clicking the label can find its window.
fn set_status_label(widgets: &Widgets, shared: &Shared, player_name: &str) {
    shared.player_name.replace(player_name.to_string());
    // Drop any instance suffix ("firefox.instance_1_88" -> "firefox")
    let short_name = player_name.split('.').next().unwrap_or(player_name);
    let label = if short_name.is_empty() {
        "NO PLAYER".to_string()
    } else {
        short_name.to_uppercase()
    };
    widgets.status_label.set_markup(&format!(
        "{}  <span letter_spacing=\"900\">{}</span>",
        mpris::player_icon(player_name),
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

/// Toggle the `paused` class, which mirrors `#mpris.paused` in the bar's stylesheet.
fn set_paused_class(widgets: &Widgets, paused: bool) {
    let ctx = widgets.window.style_context();
    if paused {
        ctx.add_class("paused");
    } else {
        ctx.remove_class("paused");
    }
}

fn refresh(widgets: &Widgets, shared: &Shared) {
    let status = mpris::playerctl(&["status"]);
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
        art::set_art(widgets, shared, "");
        widgets.seek_scale.set_sensitive(false);
        widgets.pos_label.set_text("0:00");
        widgets.dur_label.set_text("0:00");
        shared.playing.set(false);
        shared.position.set(0.0);
        shared.length.set(0.0);
        return;
    }

    let metadata = mpris::playerctl(&["metadata", "--format", mpris::METADATA_FORMAT]);
    let mut fields = metadata.split(mpris::METADATA_SEP);
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
        art::youtube_thumbnail_url(page_url)
    } else {
        art_url.to_string()
    };
    art::set_art(widgets, shared, &art_url);

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

/// Collapse a burst of signals into one refresh; a track change otherwise fires
/// several PropertiesChanged within milliseconds, each forking two playerctls.
fn queue_refresh(widgets: &Widgets, shared: &Shared) {
    if shared.refresh_queued.replace(true) {
        return;
    }
    let widgets = widgets.clone();
    let shared = shared.clone();
    glib::source::timeout_add_local_once(
        std::time::Duration::from_millis(REFRESH_COALESCE_MS),
        move || {
            shared.refresh_queued.set(false);
            refresh(&widgets, &shared);
        },
    );
}

fn subscribe_dbus(widgets: Widgets, shared: Shared) {
    let Ok(conn) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else {
        return;
    };

    {
        let widgets = widgets.clone();
        let shared = shared.clone();
        conn.signal_subscribe(
            Some(mpris::MPRIS_BUS_NAME),
            Some(mpris::DBUS_PROPERTIES_IFACE),
            Some("PropertiesChanged"),
            Some(mpris::MPRIS_OBJECT_PATH),
            None,
            gio::DBusSignalFlags::NONE,
            move |_conn, _sender, _path, _iface, _signal, _params| {
                queue_refresh(&widgets, &shared);
            },
        );
    }
    {
        // Seeks arrive as Seeked on the player interface,
        // never as a PropertiesChanged on Position.
        let widgets = widgets.clone();
        let shared = shared.clone();
        conn.signal_subscribe(
            Some(mpris::MPRIS_BUS_NAME),
            Some(mpris::MPRIS_PLAYER_IFACE),
            Some("Seeked"),
            Some(mpris::MPRIS_OBJECT_PATH),
            None,
            gio::DBusSignalFlags::NONE,
            move |_conn, _sender, _path, _iface, _signal, _params| {
                queue_refresh(&widgets, &shared);
            },
        );
    }
    {
        conn.signal_subscribe(
            Some("org.freedesktop.DBus"),
            Some("org.freedesktop.DBus"),
            Some("NameOwnerChanged"),
            Some("/org/freedesktop/DBus"),
            Some(mpris::MPRIS_BUS_NAME),
            gio::DBusSignalFlags::NONE,
            move |_conn, _sender, _path, _iface, _signal, _params| {
                queue_refresh(&widgets, &shared);
            },
        );
    }

    // Leak the connection so the subscriptions stay alive for the process's lifetime
    std::mem::forget(conn);
}

pub(crate) fn build_window() -> gtk::Window {
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.init_layer_shell();
    // Own namespace so compositor rules can target this popup alone.
    window.set_namespace(LAYER_NAMESPACE);
    window.set_layer(Layer::Overlay);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Left, true);
    window.set_layer_shell_margin(Edge::Top, POPUP_TOP_MARGIN);
    window.set_layer_shell_margin(Edge::Left, popup_left_margin());
    window.set_keyboard_mode(KeyboardMode::None);

    window.set_decorated(false);
    window.set_resizable(false);
    window.set_size_request(POPUP_WIDTH, POPUP_HEIGHT);
    window.style_context().add_class("mpris-popup");

    // An RGBA visual, so the CSS background's alpha and the area outside the
    // rounded corners render as transparent.
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
    art.set_size_request(art::ART_SIZE, art::ART_SIZE);
    art.set_valign(gtk::Align::Center);
    row.pack_start(&art, false, false, 0);

    let text_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text_box.set_valign(gtk::Align::Center);
    row.pack_start(&text_box, true, true, 0);

    let status_label = gtk::Label::new(None);
    status_label.set_xalign(0.0);
    status_label.style_context().add_class("status");
    // Ellipsizing drops the label's minimum width to one ellipsis, so set a floor.
    status_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    status_label.set_width_chars(15);

    // A button for the hover and click handling; the CSS strips its chrome so
    // it still reads as a label.
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
    // Skip buttons a size below play/pause, so the accent button leads.
    let prev_btn = make_button("media-skip-backward-symbolic", gtk::IconSize::Button);
    let playpause_btn = make_button("media-playback-start-symbolic", gtk::IconSize::LargeToolbar);
    // The accent-filled button, styled after the bar's mpris module.
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
        pending_seek: Rc::new(Cell::new(None)),
        refresh_queued: Rc::new(Cell::new(false)),
        playing: Rc::new(Cell::new(false)),
        position: Rc::new(Cell::new(0.0)),
        length: Rc::new(Cell::new(0.0)),
        art_url: Rc::new(RefCell::new(art::ART_URL_NONE.to_string())),
        art_tx,
        player_name: Rc::new(RefCell::new(String::new())),
    };

    {
        let shared = shared.clone();
        player_btn.connect_clicked(move |_| {
            let player_name = shared.player_name.borrow().clone();
            if !player_name.is_empty() && mpris::focus_player_window(&player_name) {
                // Close once the player's own window is up front.
                gtk::main_quit();
            }
        });
    }
    // Hand cursor over the label, to show it is clickable.
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
            // Several downloads can be in flight; paint only the one still wanted.
            if *shared.art_url.borrow() == url {
                art::apply_art(&widgets, &shared, &url, art::decode_art(&data));
            }
            glib::ControlFlow::Continue
        });
    }

    prev_btn.connect_clicked(|_| {
        mpris::playerctl(&["previous"]);
    });
    {
        let widgets = widgets.clone();
        let shared = shared.clone();
        playpause_btn.connect_clicked(move |_| {
            // Flip the icon first so the click feels instant; the PlaybackStatus
            // signal that follows confirms the player's real state.
            let is_playing = !shared.playing.get();
            shared.playing.set(is_playing);
            set_playpause_icon(&widgets, is_playing);
            set_paused_class(&widgets, !is_playing);

            mpris::playerctl(&["play-pause"]);
        });
    }
    next_btn.connect_clicked(|_| {
        mpris::playerctl(&["next"]);
    });

    {
        let widgets_seek = widgets.clone();
        let shared_seek = shared.clone();
        widgets.seek_scale.connect_change_value(move |_, _, value| {
            // Keep the local position in step with the drag, or the per-second
            // tick counts on from the pre-seek value and pulls the handle back.
            // Players that report no length leave the range open-ended.
            let length = shared_seek.length.get();
            let value = if length > 0.0 { value.clamp(0.0, length) } else { value.max(0.0) };
            shared_seek.position.set(value);
            widgets_seek.pos_label.set_text(&fmt_time(value));

            // Only the last value in a burst is sent; see Shared::pending_seek.
            if let Some(id) = shared_seek.pending_seek.take() {
                id.remove();
            }
            let shared_timer = shared_seek.clone();
            let id = glib::source::timeout_add_local_once(
                std::time::Duration::from_millis(SEEK_DEBOUNCE_MS),
                move || {
                    shared_timer.pending_seek.set(None);
                    send_seek(&shared_timer);
                },
            );
            shared_seek.pending_seek.set(Some(id));
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
            // Don't make the release wait out the debounce.
            send_seek(&shared);
            glib::Propagation::Proceed
        });
    }

    // Local per-second tick to advance the seek bar between D-Bus events;
    // MPRIS has no per-second position signal.
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
