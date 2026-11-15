// Popup window: widgets, D-Bus subscriptions and refresh.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk::prelude::*;
use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::art;
use crate::audio;
use crate::mpris;

const HIDE_DELAY_MS: u64 = 3000;
/// How long the drag has to settle before the seek is actually sent.
const SEEK_DEBOUNCE_MS: u64 = 80;
/// How long to let a burst of D-Bus signals settle before refreshing.
const REFRESH_COALESCE_MS: u64 = 30;

const POPUP_WIDTH: i32 = 380;
/// Volume side panel: width, gap from the popup, and hover grace period.
const PANEL_WIDTH: i32 = 56;
const PANEL_GAP: i32 = 8;
const PANEL_HIDE_MS: u64 = 300;
/// Percent per scroll notch.
const VOLUME_SCROLL_STEP: f64 = 1.0;
/// Percent per left/right click on the speaker.
const VOLUME_CLICK_STEP: f64 = 10.0;
const VOLUME_MAX_PCT: f64 = audio::MAX_VOLUME * 100.0;
const POPUP_HEIGHT: i32 = 120;
/// Gap between the top of the screen and the popup
const POPUP_TOP_MARGIN: i32 = 10;
/// Fallback screen-left offset, used when the host file below is missing
const POPUP_LEFT_MARGIN_DEFAULT: i32 = 275;
/// Per-machine left offset, from config/hosts/<profile>/mpris-popup-margin
const POPUP_LEFT_MARGIN_PATH: &str = "~/.config/waybar/mpris-popup-margin";

/// Wayland layer-surface namespace; what compositor layer rules match on
const LAYER_NAMESPACE: &str = "mpris-popup";
const PANEL_NAMESPACE: &str = "mpris-popup-volume";

/// Per-host left margin, or the default if missing or unparseable.
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
    let pos = shared.position.get().to_string();
    mpris::playerctl_for(&shared.main_player.borrow(), &["position", &pos]);
}

fn fmt_time(seconds: f64) -> String {
    let seconds: i64 = seconds.max(0.0) as i64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

#[derive(Clone)]
struct Widgets {
    window: gtk::Window,
    art: art::Art,
    status_label: gtk::Label,
    title_label: gtk::Label,
    artist_label: gtk::Label,
    pos_label: gtk::Label,
    dur_label: gtk::Label,
    seek_scale: gtk::Scale,
    prev_btn: gtk::Button,
    playpause_btn: gtk::Button,
    next_btn: gtk::Button,
    vol_btn: gtk::Button,
    /// Side panel, its own layer surface: a Popover can't leave the popup.
    vol_panel: gtk::Window,
    vol_label: gtk::Label,
    vol_scale: gtk::Scale,
    vol_mute_btn: gtk::Button,
    separator: gtk::Separator,
    mini_box: gtk::Box,
}

/// Compact row for a player that isn't in the main card.
struct MiniRow {
    row: gtk::Box,
    label: gtk::Label,
    playpause: gtk::Button,
}

#[derive(Clone)]
struct Shared {
    seeking: Rc<Cell<bool>>,
    /// Debounces seeks: playerctl blocks the main loop, change-value fires per frame.
    pending_seek: Rc<Cell<Option<glib::SourceId>>>,
    /// Set while a refresh is scheduled, so a burst of signals costs one refresh.
    refresh_queued: Rc<Cell<bool>>,
    playing: Rc<Cell<bool>>,
    position: Rc<Cell<f64>>,
    length: Rc<Cell<f64>>,
    main_player: Rc<RefCell<String>>,
    /// Set when a row was clicked: keep that player over whatever is playing.
    pinned: Rc<Cell<bool>>,
    rows: Rc<RefCell<HashMap<String, MiniRow>>>,
    /// Main player's audio streams, as of the last refresh.
    streams: Rc<RefCell<Vec<audio::Stream>>>,
    vol_dragging: Rc<Cell<bool>>,
    /// Debounces volume changes, like `pending_seek`.
    pending_vol: Rc<Cell<Option<glib::SourceId>>>,
    /// Grace timer before the volume panel hides.
    panel_hide: Rc<Cell<Option<glib::SourceId>>>,
    panel_hovered: Rc<Cell<bool>>,
}

/// Send the slider's volume now.
fn send_volume(widgets: &Widgets, shared: &Shared) {
    if let Some(id) = shared.pending_vol.take() {
        id.remove();
    }
    audio::set_volume(&shared.streams.borrow(), widgets.vol_scale.value() / 100.0);
}

/// Label `pct` now (the slider lags change-value), send once it settles.
fn queue_volume_send(widgets: &Widgets, shared: &Shared, pct: f64) {
    widgets.vol_label.set_text(&format!("{}%", pct.round()));
    if let Some(id) = shared.pending_vol.take() {
        id.remove();
    }
    let (widgets_timer, shared_timer) = (widgets.clone(), shared.clone());
    let id = glib::source::timeout_add_local_once(
        std::time::Duration::from_millis(SEEK_DEBOUNCE_MS),
        move || {
            shared_timer.pending_vol.set(None);
            send_volume(&widgets_timer, &shared_timer);
        },
    );
    shared.pending_vol.set(Some(id));
}

fn scroll_volume(widgets: &Widgets, shared: &Shared, ev: &gdk::EventScroll) {
    let notches = match ev.direction() {
        gdk::ScrollDirection::Up => 1.0,
        gdk::ScrollDirection::Down => -1.0,
        gdk::ScrollDirection::Smooth => -ev.delta().1,
        _ => 0.0,
    };
    if notches != 0.0 {
        nudge_volume(widgets, shared, notches * VOLUME_SCROLL_STEP);
    }
}

/// Shift the volume by `delta` percent.
fn nudge_volume(widgets: &Widgets, shared: &Shared, delta: f64) {
    if shared.streams.borrow().is_empty() {
        return;
    }
    let value = (widgets.vol_scale.value() + delta).clamp(0.0, VOLUME_MAX_PCT);
    widgets.vol_scale.set_value(value);
    queue_volume_send(widgets, shared, value);
}

fn toggle_mute(widgets: &Widgets, shared: &Shared) {
    audio::toggle_mute(&shared.streams.borrow());
    queue_refresh(widgets, shared);
}

/// Line the panel up to the popup's right edge, matching its height.
fn place_panel(widgets: &Widgets) {
    let (w, h) = (widgets.window.allocated_width(), widgets.window.allocated_height());
    let panel = &widgets.vol_panel;
    panel.set_layer_shell_margin(Edge::Left, popup_left_margin() + w + PANEL_GAP);
    panel.set_size_request(PANEL_WIDTH, h);
    panel.resize(PANEL_WIDTH, h);
}

fn show_volume_panel(widgets: &Widgets, shared: &Shared) {
    if let Some(id) = shared.panel_hide.take() {
        id.remove();
    }
    if widgets.vol_panel.is_visible() || !widgets.vol_btn.is_sensitive() {
        return;
    }
    place_panel(widgets);
    widgets.vol_panel.show_all();
}

fn schedule_panel_hide(widgets: &Widgets, shared: &Shared) {
    if shared.vol_dragging.get() {
        return;
    }
    if let Some(id) = shared.panel_hide.take() {
        id.remove();
    }
    let (panel, shared_timer) = (widgets.vol_panel.clone(), shared.clone());
    let id = glib::source::timeout_add_local_once(
        std::time::Duration::from_millis(PANEL_HIDE_MS),
        move || {
            shared_timer.panel_hide.set(None);
            panel.hide();
        },
    );
    shared.panel_hide.set(Some(id));
}

/// Transparent RGBA visual, so rounded corners render without a backdrop.
fn use_rgba_visual(window: &gtk::Window) {
    if let Some(visual) = WidgetExt::screen(window).and_then(|s| s.rgba_visual()) {
        window.set_visual(Some(&visual));
    }
}

fn build_volume_panel() -> (gtk::Window, gtk::Label, gtk::Scale, gtk::Button) {
    let panel = gtk::Window::new(gtk::WindowType::Toplevel);
    panel.init_layer_shell();
    panel.set_namespace(PANEL_NAMESPACE);
    panel.set_layer(Layer::Overlay);
    panel.set_anchor(Edge::Top, true);
    panel.set_anchor(Edge::Left, true);
    panel.set_layer_shell_margin(Edge::Top, POPUP_TOP_MARGIN);
    panel.set_keyboard_mode(KeyboardMode::None);
    panel.set_decorated(false);
    panel.set_resizable(false);
    panel.style_context().add_class("mpris-volume-panel");
    use_rgba_visual(&panel);

    let col = gtk::Box::new(gtk::Orientation::Vertical, 6);
    col.set_margin_top(12);
    col.set_margin_bottom(8);
    panel.add(&col);

    let label = gtk::Label::new(Some("0%"));
    label.style_context().add_class("vol-label");
    col.pack_start(&label, false, false, 0);

    let scale = gtk::Scale::with_range(gtk::Orientation::Vertical, 0.0, VOLUME_MAX_PCT, 1.0);
    scale.set_inverted(true);
    // Tick at 100%: above it is software boost.
    scale.add_mark(100.0, gtk::PositionType::Right, None);
    scale.set_draw_value(false);
    scale.set_vexpand(true);
    scale.set_halign(gtk::Align::Center);
    scale.style_context().add_class("mpris-volume");
    col.pack_start(&scale, true, true, 0);

    let mute = gtk::Button::new();
    mute.set_relief(gtk::ReliefStyle::None);
    mute.set_halign(gtk::Align::Center);
    mute.style_context().add_class("mpris-btn");
    mute.style_context().add_class("vol-btn");
    col.pack_start(&mute, false, false, 0);

    (panel, label, scale, mute)
}

fn volume_icon(volume: f64, muted: bool) -> &'static str {
    match volume {
        _ if muted || volume <= 0.0 => "audio-volume-muted-symbolic",
        v if v < 0.34 => "audio-volume-low-symbolic",
        v if v < 0.67 => "audio-volume-medium-symbolic",
        _ => "audio-volume-high-symbolic",
    }
}

fn refresh_volume(widgets: &Widgets, shared: &Shared, instance: &str) {
    let streams = if instance.is_empty() { Vec::new() } else { audio::streams_for(instance) };
    let first = streams.first().map(|s| (s.volume, s.muted));
    shared.streams.replace(streams);

    let (volume, muted) = first.unwrap_or((0.0, true));
    for btn in [&widgets.vol_btn, &widgets.vol_mute_btn] {
        btn.set_image(Some(&gtk::Image::from_icon_name(
            Some(volume_icon(volume, muted)),
            gtk::IconSize::Button,
        )));
    }
    // No stream: player is silent (e.g. paused Firefox), nothing to adjust.
    widgets.vol_btn.set_sensitive(first.is_some());
    if first.is_none() {
        widgets.vol_panel.hide();
        return;
    }

    // Don't yank the handle mid-drag or before a pending change is sent.
    let pending = shared.pending_vol.take();
    let busy = pending.is_some() || shared.vol_dragging.get();
    shared.pending_vol.set(pending);
    if !busy {
        widgets.vol_scale.set_value(volume * 100.0);
        widgets.vol_label.set_text(&if muted {
            "Muted".to_string()
        } else {
            format!("{}%", (volume * 100.0).round())
        });
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

/// True when the pointer left the popup, not just moved onto a child.
fn is_pointer_exit(ev: &gdk::EventCrossing) -> bool {
    ev.detail() != gdk::NotifyType::Inferior
}

/// "{player icon}  {PLAYER NAME}", like the bar module; also records the player.
fn set_status_label(widgets: &Widgets, shared: &Shared, player_name: &str) {
    shared.main_player.replace(player_name.to_string());
    let short_name = mpris::short_name(player_name);
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

fn set_playpause_image(btn: &gtk::Button, is_playing: bool, size: gtk::IconSize) {
    let icon = if is_playing {
        "media-playback-pause-symbolic"
    } else {
        "media-playback-start-symbolic"
    };
    btn.set_image(Some(&gtk::Image::from_icon_name(Some(icon), size)));
}

fn set_playpause_icon(widgets: &Widgets, is_playing: bool) {
    set_playpause_image(&widgets.playpause_btn, is_playing, gtk::IconSize::LargeToolbar);
}

fn set_class(widget: &impl IsA<gtk::Widget>, class: &str, on: bool) {
    let ctx = widget.style_context();
    if on {
        ctx.add_class(class);
    } else {
        ctx.remove_class(class);
    }
}

/// Toggle the `paused` class, which mirrors `#mpris.paused` in the bar's stylesheet.
fn set_paused_class(widgets: &Widgets, paused: bool) {
    set_class(&widgets.window, "paused", paused);
}

/// Pinned player, else the most recent playing one, else current, active, any.
fn pick_main<'a>(shared: &Shared, players: &'a [mpris::PlayerInfo]) -> Option<&'a mpris::PlayerInfo> {
    let find = |name: &str| players.iter().find(|p| p.instance == name);
    let current = shared.main_player.borrow().clone();

    if shared.pinned.get() {
        if let Some(p) = find(&current) {
            return Some(p);
        }
        shared.pinned.set(false);
    }

    let active = mpris::active_player();
    let playing = |p: &&mpris::PlayerInfo| p.is_playing();
    // playerctld's order can still put a paused player first.
    active
        .as_deref()
        .and_then(find)
        .filter(playing)
        .or_else(|| players.iter().find(playing))
        .or_else(|| find(&current))
        .or_else(|| active.as_deref().and_then(find))
        .or(players.first())
}

fn build_mini_row(widgets: &Widgets, shared: &Shared, instance: &str) -> MiniRow {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    row.style_context().add_class("mini-row");

    let label = gtk::Label::new(None);
    label.set_xalign(0.0);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    // Tiny natural width so long titles ellipsize instead of widening the popup.
    label.set_max_width_chars(1);
    label.set_hexpand(true);
    label.style_context().add_class("mini-title");

    // Button so the whole row is a click target; promotes the player.
    let promote = gtk::Button::new();
    promote.set_relief(gtk::ReliefStyle::None);
    promote.set_hexpand(true);
    promote.style_context().add_class("mini-link");
    promote.add(&label);
    row.pack_start(&promote, true, true, 0);

    let playpause = gtk::Button::new();
    playpause.set_relief(gtk::ReliefStyle::None);
    playpause.style_context().add_class("mpris-btn");
    playpause.style_context().add_class("mini-btn");
    row.pack_start(&playpause, false, false, 0);

    {
        let widgets = widgets.clone();
        let shared = shared.clone();
        let instance = instance.to_string();
        promote.connect_clicked(move |_| {
            shared.main_player.replace(instance.clone());
            shared.pinned.set(true);
            queue_refresh(&widgets, &shared);
        });
    }
    {
        let instance = instance.to_string();
        playpause.connect_clicked(move |_| {
            mpris::playerctl_for(&instance, &["play-pause"]);
        });
    }
    {
        let window = widgets.window.clone();
        promote.connect_enter_notify_event(move |_, _| {
            set_cursor(&window, Some("pointer"));
            glib::Propagation::Proceed
        });
    }
    {
        let window = widgets.window.clone();
        promote.connect_leave_notify_event(move |_, _| {
            set_cursor(&window, None);
            glib::Propagation::Proceed
        });
    }

    MiniRow { row, label, playpause }
}

/// Diff the mini rows against every player but the main one.
fn sync_rows(widgets: &Widgets, shared: &Shared, players: &[mpris::PlayerInfo], main: &str) {
    let others: Vec<&mpris::PlayerInfo> = players.iter().filter(|p| p.instance != main).collect();
    let mut rows = shared.rows.borrow_mut();
    let before = rows.len();

    rows.retain(|name, row| {
        let keep = others.iter().any(|p| &p.instance == name);
        if !keep {
            widgets.mini_box.remove(&row.row);
        }
        keep
    });

    for p in &others {
        let row = rows.entry(p.instance.clone()).or_insert_with(|| {
            let row = build_mini_row(widgets, shared, &p.instance);
            widgets.mini_box.pack_start(&row.row, false, false, 0);
            row.row.show_all();
            row
        });
        let title = if p.title.is_empty() { "Unknown title" } else { &p.title };
        row.label.set_markup(&format!(
            "{}  <b>{}</b>  {}",
            mpris::player_icon(&p.instance),
            glib::markup_escape_text(&mpris::short_name(&p.instance).to_uppercase()),
            glib::markup_escape_text(title),
        ));
        set_playpause_image(&row.playpause, p.is_playing(), gtk::IconSize::Button);
        set_class(&row.row, "paused", !p.is_playing());
    }

    let has_rows = !rows.is_empty();
    widgets.separator.set_visible(has_rows);
    widgets.mini_box.set_visible(has_rows);
    // Layer-shell windows don't shrink on their own.
    if rows.len() < before {
        widgets.window.resize(1, 1);
    }
}

fn refresh(widgets: &Widgets, shared: &Shared) {
    let players = mpris::list_players();
    let main = pick_main(shared, &players);
    let main_name = main.map_or("", |p| p.instance.as_str());
    sync_rows(widgets, shared, &players, main_name);
    refresh_volume(widgets, shared, main_name);

    let has_player = main.is_some();
    for b in [&widgets.prev_btn, &widgets.playpause_btn, &widgets.next_btn] {
        b.set_sensitive(has_player);
    }

    let Some(info) = main else {
        set_status_label(widgets, shared, "");
        set_paused_class(widgets, true);
        set_playpause_icon(widgets, false);
        widgets.title_label.set_text("No media player");
        widgets.artist_label.set_text("");
        widgets.art.set("");
        widgets.seek_scale.set_sensitive(false);
        widgets.pos_label.set_text("0:00");
        widgets.dur_label.set_text("0:00");
        shared.playing.set(false);
        shared.position.set(0.0);
        shared.length.set(0.0);
        return;
    };

    set_status_label(widgets, shared, &info.instance);

    let title = if info.title.is_empty() { "Unknown title" } else { &info.title };
    widgets
        .title_label
        .set_markup(&format!("<b>{}</b>", glib::markup_escape_text(title)));
    widgets.artist_label.set_text(&info.artist);

    let is_playing = info.is_playing();
    set_paused_class(widgets, !is_playing);
    set_playpause_icon(widgets, is_playing);

    let art_url = if info.art_url.is_empty() {
        // Firefox gives no artUrl for YouTube; derive it from the page URL.
        art::youtube_thumbnail_url(&info.page_url)
    } else {
        info.art_url.clone()
    };
    widgets.art.set(&art_url);

    let (length, position) = (info.length, info.position);

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

/// Collapse a burst of signals (e.g. a track change) into one refresh.
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

    // Any sender: every player, not just playerctld's proxied one.
    {
        let widgets = widgets.clone();
        let shared = shared.clone();
        conn.signal_subscribe(
            None,
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
        // Seeks come as Seeked, never as PropertiesChanged on Position.
        let widgets = widgets.clone();
        let shared = shared.clone();
        conn.signal_subscribe(
            None,
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
        // Players appearing or quitting.
        conn.signal_subscribe(
            Some("org.freedesktop.DBus"),
            Some("org.freedesktop.DBus"),
            Some("NameOwnerChanged"),
            Some("/org/freedesktop/DBus"),
            Some(mpris::MPRIS_BUS_PREFIX.trim_end_matches('.')),
            gio::DBusSignalFlags::MATCH_ARG0_NAMESPACE,
            move |_conn, _sender, _path, _iface, _signal, _params| {
                queue_refresh(&widgets, &shared);
            },
        );
    }

    // Leak the connection so the subscriptions stay alive for the process's lifetime
    std::mem::forget(conn);
}

/// The window, plus the `pactl subscribe` child for the caller to kill on exit.
pub(crate) fn build_window() -> (gtk::Window, Option<std::process::Child>) {
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

    use_rgba_visual(&window);

    let outer = gtk::Box::new(gtk::Orientation::Vertical, 10);
    outer.set_margin_top(14);
    outer.set_margin_bottom(12);
    outer.set_margin_start(16);
    outer.set_margin_end(16);
    window.add(&outer);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    outer.pack_start(&row, true, true, 0);

    let art = art::Art::new();
    row.pack_start(art.widget(), false, false, 0);

    let text_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text_box.set_valign(gtk::Align::Center);
    row.pack_start(&text_box, true, true, 0);

    let status_label = gtk::Label::new(None);
    status_label.set_xalign(0.0);
    status_label.style_context().add_class("status");
    // Ellipsizing drops the label's minimum width to one ellipsis, so set a floor.
    status_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    status_label.set_width_chars(15);

    // Button for hover/click; CSS makes it look like a label.
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

    // Center widget keeps the transport buttons centred.
    let controls_row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    outer.pack_start(&controls_row, false, false, 0);
    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    controls_row.set_center_widget(Some(&controls));

    // Hover opens the side panel; clicks and scroll adjust.
    let vol_btn = gtk::Button::new();
    vol_btn.set_relief(gtk::ReliefStyle::None);
    vol_btn.set_valign(gtk::Align::Center);
    vol_btn.style_context().add_class("mpris-btn");
    vol_btn.style_context().add_class("vol-btn");
    vol_btn.add_events(gdk::EventMask::SCROLL_MASK | gdk::EventMask::SMOOTH_SCROLL_MASK);
    controls_row.pack_end(&vol_btn, false, false, 0);
    let (vol_panel, vol_label, vol_scale, vol_mute_btn) = build_volume_panel();

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

    // Other players; refresh decides visibility, so show_all mustn't.
    let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
    separator.set_no_show_all(true);
    outer.pack_start(&separator, false, false, 0);
    let mini_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    mini_box.set_no_show_all(true);
    outer.pack_start(&mini_box, false, false, 0);

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
        vol_btn,
        vol_panel,
        vol_label,
        vol_scale,
        vol_mute_btn,
        separator,
        mini_box,
    };
    let shared = Shared {
        seeking: Rc::new(Cell::new(false)),
        pending_seek: Rc::new(Cell::new(None)),
        refresh_queued: Rc::new(Cell::new(false)),
        playing: Rc::new(Cell::new(false)),
        position: Rc::new(Cell::new(0.0)),
        length: Rc::new(Cell::new(0.0)),
        main_player: Rc::new(RefCell::new(String::new())),
        pinned: Rc::new(Cell::new(false)),
        rows: Rc::new(RefCell::new(HashMap::new())),
        streams: Rc::new(RefCell::new(Vec::new())),
        vol_dragging: Rc::new(Cell::new(false)),
        pending_vol: Rc::new(Cell::new(None)),
        panel_hide: Rc::new(Cell::new(None)),
        panel_hovered: Rc::new(Cell::new(false)),
    };

    {
        let (widgets, shared) = (widgets.clone(), shared.clone());
        widgets.vol_mute_btn.clone().connect_clicked(move |_| {
            toggle_mute(&widgets, &shared);
        });
    }
    // Speaker: left +10%, right -10%, middle mutes.
    {
        let (widgets, shared) = (widgets.clone(), shared.clone());
        widgets.vol_btn.clone().connect_button_press_event(move |_, ev| {
            match ev.button() {
                1 => nudge_volume(&widgets, &shared, VOLUME_CLICK_STEP),
                2 => toggle_mute(&widgets, &shared),
                3 => nudge_volume(&widgets, &shared, -VOLUME_CLICK_STEP),
                _ => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        });
    }
    {
        let (widgets, shared) = (widgets.clone(), shared.clone());
        widgets.vol_btn.clone().connect_enter_notify_event(move |_, _| {
            show_volume_panel(&widgets, &shared);
            glib::Propagation::Proceed
        });
    }
    {
        let (widgets, shared) = (widgets.clone(), shared.clone());
        widgets.vol_btn.clone().connect_leave_notify_event(move |_, _| {
            schedule_panel_hide(&widgets, &shared);
            glib::Propagation::Proceed
        });
    }
    // Same step on the speaker and the slider; overrides GtkScale's own wheel step.
    for w in [widgets.vol_btn.upcast_ref::<gtk::Widget>(), widgets.vol_scale.upcast_ref()] {
        let (widgets, shared) = (widgets.clone(), shared.clone());
        w.connect_scroll_event(move |_, ev| {
            scroll_volume(&widgets, &shared, ev);
            glib::Propagation::Stop
        });
    }
    {
        let (widgets, shared) = (widgets.clone(), shared.clone());
        widgets.vol_panel.clone().connect_enter_notify_event(move |_, _| {
            shared.panel_hovered.set(true);
            show_volume_panel(&widgets, &shared);
            glib::Propagation::Proceed
        });
    }
    {
        let (widgets, shared) = (widgets.clone(), shared.clone());
        widgets.vol_panel.clone().connect_leave_notify_event(move |_, ev| {
            if is_pointer_exit(ev) {
                shared.panel_hovered.set(false);
                schedule_panel_hide(&widgets, &shared);
            }
            glib::Propagation::Proceed
        });
    }
    // Keep the panel matched to the popup's height.
    {
        let widgets = widgets.clone();
        window.connect_size_allocate(move |_, _| {
            if widgets.vol_panel.is_visible() {
                place_panel(&widgets);
            }
        });
    }
    {
        let (widgets_vol, shared_vol) = (widgets.clone(), shared.clone());
        widgets.vol_scale.connect_change_value(move |_, _, value| {
            queue_volume_send(&widgets_vol, &shared_vol, value.clamp(0.0, VOLUME_MAX_PCT));
            glib::Propagation::Proceed
        });
    }
    {
        let shared = shared.clone();
        widgets.vol_scale.connect_button_press_event(move |_, _| {
            shared.vol_dragging.set(true);
            glib::Propagation::Proceed
        });
    }
    {
        let (widgets_rel, shared) = (widgets.clone(), shared.clone());
        widgets.vol_scale.connect_button_release_event(move |_, _| {
            shared.vol_dragging.set(false);
            send_volume(&widgets_rel, &shared);
            // Pointer may have drifted off the panel mid-drag.
            if !shared.panel_hovered.get() {
                schedule_panel_hide(&widgets_rel, &shared);
            }
            glib::Propagation::Proceed
        });
    }
    // External changes (pavucontrol, keys) show up as sink-input events.
    #[allow(deprecated)]
    let (vol_tx, vol_rx) = glib::MainContext::channel::<()>(glib::Priority::DEFAULT);
    let subscriber = audio::spawn_subscribe(vol_tx);
    {
        let (widgets, shared) = (widgets.clone(), shared.clone());
        vol_rx.attach(None, move |()| {
            queue_refresh(&widgets, &shared);
            glib::ControlFlow::Continue
        });
    }

    {
        let shared = shared.clone();
        player_btn.connect_clicked(move |_| {
            let player_name = shared.main_player.borrow().clone();
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
        let shared = shared.clone();
        prev_btn.connect_clicked(move |_| {
            mpris::playerctl_for(&shared.main_player.borrow(), &["previous"]);
        });
    }
    {
        let widgets = widgets.clone();
        let shared = shared.clone();
        playpause_btn.connect_clicked(move |_| {
            // Flip now so it feels instant; PlaybackStatus confirms later.
            let is_playing = !shared.playing.get();
            shared.playing.set(is_playing);
            set_playpause_icon(&widgets, is_playing);
            set_paused_class(&widgets, !is_playing);

            mpris::playerctl_for(&shared.main_player.borrow(), &["play-pause"]);
        });
    }
    {
        let shared = shared.clone();
        next_btn.connect_clicked(move |_| {
            mpris::playerctl_for(&shared.main_player.borrow(), &["next"]);
        });
    }

    {
        let widgets_seek = widgets.clone();
        let shared_seek = shared.clone();
        widgets.seek_scale.connect_change_value(move |_, _, value| {
            // Track the drag, or the per-second tick pulls the handle back.
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

    // Per-second tick; MPRIS has no position signal.
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

    // The panel counts as part of the popup: hovering it keeps both open.
    for win in [&window, &widgets.vol_panel] {
        {
            let cancel_hide_timer = cancel_hide_timer.clone();
            win.connect_enter_notify_event(move |_, _| {
                cancel_hide_timer();
                glib::Propagation::Proceed
            });
        }
        {
            let start_hide_timer = start_hide_timer.clone();
            win.connect_leave_notify_event(move |_, ev| {
                if is_pointer_exit(ev) {
                    start_hide_timer();
                }
                glib::Propagation::Proceed
            });
        }
    }

    refresh(&widgets, &shared);
    subscribe_dbus(widgets, shared);
    start_hide_timer();

    (window, subscriber)
}
