#!/usr/bin/env python3

# Author: JialaChang
# Floating GTK popup for waybar's mpris module (click-to-open, since waybar
# has no hover-exec): shows cover art, title/artist, a seek bar, and
# prev/play-pause/next controls, driven through playerctl. Refreshes are
# triggered by D-Bus PropertiesChanged signals from playerctld rather than
# polling; only the seek bar's per-second advance is a local timer.

import os
import re
import signal
import subprocess
import sys
import urllib.request

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
gi.require_version("GdkPixbuf", "2.0")
gi.require_version("Gio", "2.0")
gi.require_version("GtkLayerShell", "0.1")
from gi.repository import Gdk, GdkPixbuf, Gio, GLib, Gtk, GtkLayerShell, Pango

PIDFILE = "/tmp/waybar-mpris-popup.pid"
CSS_PATH = os.path.expanduser("~/.config/waybar/mpris-popup.css")
ART_SIZE = 88
HIDE_DELAY_MS = 3000
POPUP_WIDTH = 420
POPUP_HEIGHT = 190
# Gap between the top of the screen and the popup (i.e. distance below the bar)
POPUP_TOP_MARGIN = 10
# Approximate screen-left offset of the mpris module in modules-left
POPUP_LEFT_MARGIN = 230

# playerctld proxies whichever player is currently active under one fixed bus name
MPRIS_BUS_NAME = "org.mpris.MediaPlayer2.playerctld"
MPRIS_OBJECT_PATH = "/org/mpris/MediaPlayer2"
DBUS_PROPERTIES_IFACE = "org.freedesktop.DBus.Properties"


def playerctl(*args):
    try:
        return subprocess.check_output(
            ["playerctl", *args], text=True, stderr=subprocess.DEVNULL
        ).strip()
    except subprocess.CalledProcessError:
        return ""


def already_running():
    if os.path.exists(PIDFILE):
        try:
            pid = int(open(PIDFILE).read().strip())
            os.kill(pid, 0)
            return pid
        except (OSError, ValueError):
            pass
    return None


class Popup(Gtk.Window):
    def __init__(self):
        super().__init__()
        GtkLayerShell.init_for_window(self)
        GtkLayerShell.set_layer(self, GtkLayerShell.Layer.OVERLAY)
        GtkLayerShell.set_anchor(self, GtkLayerShell.Edge.TOP, True)
        GtkLayerShell.set_anchor(self, GtkLayerShell.Edge.LEFT, True)
        GtkLayerShell.set_margin(self, GtkLayerShell.Edge.TOP, POPUP_TOP_MARGIN)
        GtkLayerShell.set_margin(self, GtkLayerShell.Edge.LEFT, POPUP_LEFT_MARGIN)
        GtkLayerShell.set_keyboard_mode(self, GtkLayerShell.KeyboardMode.NONE)

        self.set_decorated(False)
        self.set_resizable(False)
        self.set_size_request(POPUP_WIDTH, POPUP_HEIGHT)
        self.get_style_context().add_class("mpris-popup")

        # Needed so the CSS background's alpha channel (and the area outside
        # the rounded corners) actually renders as transparent instead of an opaque box.
        screen = self.get_screen()
        visual = screen.get_rgba_visual() if screen else None
        if visual:
            self.set_visual(visual)

        outer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        outer.set_margin_top(14)
        outer.set_margin_bottom(14)
        outer.set_margin_start(16)
        outer.set_margin_end(16)
        self.add(outer)

        row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        outer.pack_start(row, True, True, 0)

        self.art = Gtk.Image()
        self.art.set_size_request(ART_SIZE, ART_SIZE)
        row.pack_start(self.art, False, False, 0)

        text_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        text_box.set_valign(Gtk.Align.CENTER)
        row.pack_start(text_box, True, True, 0)

        self.title_label = Gtk.Label(xalign=0)
        self.title_label.get_style_context().add_class("title")
        self.title_label.set_line_wrap(True)
        self.title_label.set_line_wrap_mode(Pango.WrapMode.WORD_CHAR)
        self.title_label.set_max_width_chars(30)
        text_box.pack_start(self.title_label, False, False, 0)

        self.artist_label = Gtk.Label(xalign=0)
        self.artist_label.get_style_context().add_class("artist")
        self.artist_label.set_ellipsize(Pango.EllipsizeMode.END)
        self.artist_label.set_max_width_chars(24)
        text_box.pack_start(self.artist_label, False, False, 0)

        seek_row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=6)
        outer.pack_start(seek_row, False, False, 0)

        self.pos_label = Gtk.Label(label="0:00")
        self.pos_label.get_style_context().add_class("time")
        seek_row.pack_start(self.pos_label, False, False, 0)

        self.seek_scale = Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL, 0, 1, 1)
        self.seek_scale.set_draw_value(False)
        self.seek_scale.set_hexpand(True)
        self.seek_scale.get_style_context().add_class("mpris-seek")
        self.seek_scale.connect("change-value", self._on_seek)
        self.seek_scale.connect("button-press-event", lambda *_: setattr(self, "_seeking", True))
        self.seek_scale.connect("button-release-event", lambda *_: setattr(self, "_seeking", False))
        seek_row.pack_start(self.seek_scale, True, True, 0)

        self.dur_label = Gtk.Label(label="0:00")
        self.dur_label.get_style_context().add_class("time")
        seek_row.pack_start(self.dur_label, False, False, 0)

        self._seeking = False

        controls = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        controls.set_halign(Gtk.Align.CENTER)
        outer.pack_start(controls, False, False, 0)

        self.prev_btn = self._make_button(
            "media-skip-backward-symbolic", lambda *_: playerctl("previous")
        )
        self.playpause_btn = self._make_button(
            "media-playback-start-symbolic", lambda *_: self._on_playpause()
        )
        self.next_btn = self._make_button(
            "media-skip-forward-symbolic", lambda *_: playerctl("next")
        )
        for b in (self.prev_btn, self.playpause_btn, self.next_btn):
            controls.pack_start(b, False, False, 0)

        self.connect("enter-notify-event", lambda *_: self._cancel_hide_timer())
        self.connect("leave-notify-event", lambda *_: self._start_hide_timer())

        self._hide_timer = None
        self._status = ""
        self._position = 0.0
        self._length = 0.0

        self.refresh()
        self._subscribe_dbus()
        GLib.timeout_add(1000, self._tick)
        self._start_hide_timer()

    def _subscribe_dbus(self):
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        bus.signal_subscribe(
            MPRIS_BUS_NAME,
            DBUS_PROPERTIES_IFACE,
            "PropertiesChanged",
            MPRIS_OBJECT_PATH,
            None,
            Gio.DBusSignalFlags.NONE,
            self._on_properties_changed,
        )
        # Catches the player appearing/disappearing (playerctld only owns
        # the name while a player is active).
        bus.signal_subscribe(
            "org.freedesktop.DBus",
            "org.freedesktop.DBus",
            "NameOwnerChanged",
            "/org/freedesktop/DBus",
            MPRIS_BUS_NAME,
            Gio.DBusSignalFlags.NONE,
            self._on_properties_changed,
        )

    def _on_properties_changed(self, *_args):
        self.refresh()

    def _make_button(self, icon_name, callback):
        btn = Gtk.Button()
        btn.set_image(Gtk.Image.new_from_icon_name(icon_name, Gtk.IconSize.LARGE_TOOLBAR))
        btn.get_style_context().add_class("mpris-btn")
        btn.connect("clicked", callback)
        return btn

    def _on_playpause(self):
        playerctl("play-pause")
        GLib.timeout_add(50, self._refresh_once)

    def _refresh_once(self):
        self.refresh()
        return False

    def _on_seek(self, _range, _scroll, value):
        playerctl("position", str(value))
        return False

    @staticmethod
    def _fmt_time(seconds):
        seconds = max(0, int(seconds))
        m, s = divmod(seconds, 60)
        return f"{m}:{s:02d}"

    def _start_hide_timer(self):
        self._cancel_hide_timer()
        self._hide_timer = GLib.timeout_add(HIDE_DELAY_MS, self._hide)

    def _cancel_hide_timer(self):
        if self._hide_timer:
            GLib.source_remove(self._hide_timer)
            self._hide_timer = None

    def _hide(self):
        Gtk.main_quit()
        return False

    def _tick(self):
        # Runs every second purely to advance the seek bar locally between
        # real D-Bus events — MPRIS only emits PropertiesChanged/Seeked on
        # actual state changes, not a per-second position tick, so without
        # this the bar would only move when something else happens to
        # trigger a refresh(). No playerctl/D-Bus calls happen here.
        if not self._seeking and self._status == "Playing" and self._length > 0:
            self._position = min(self._position + 1, self._length)
            self.seek_scale.set_value(self._position)
            self.pos_label.set_text(self._fmt_time(self._position))
        return True

    def refresh(self):
        status = playerctl("status")
        has_player = bool(status)
        for b in (self.prev_btn, self.playpause_btn, self.next_btn):
            b.set_sensitive(has_player)

        if not has_player:
            self.title_label.set_text("No media player")
            self.artist_label.set_text("")
            self.art.set_from_icon_name("audio-x-generic-symbolic", Gtk.IconSize.DIALOG)
            self.seek_scale.set_sensitive(False)
            self.pos_label.set_text("0:00")
            self.dur_label.set_text("0:00")
            self._status = ""
            self._position = 0.0
            self._length = 0.0
            return True

        title = playerctl("metadata", "title") or "Unknown title"
        artist = playerctl("metadata", "artist")
        self.title_label.set_markup(f"<b>{GLib.markup_escape_text(title)}</b>")
        self.artist_label.set_text(artist)

        icon = "media-playback-pause-symbolic" if status == "Playing" else "media-playback-start-symbolic"
        self.playpause_btn.set_image(Gtk.Image.new_from_icon_name(icon, Gtk.IconSize.LARGE_TOOLBAR))

        art_url = playerctl("metadata", "mpris:artUrl")
        if not art_url:
            # Firefox doesn't expose mpris:artUrl for YouTube;
            # derive a thumbnail from the page URL instead.
            art_url = self._youtube_thumbnail_url(playerctl("metadata", "xesam:url"))
        self._set_art(art_url)

        length_str = playerctl("metadata", "mpris:length")
        # the unit of mpris:length is μs
        length = int(length_str) / 1_000_000 if length_str.isdigit() else 0
        try:
            position = float(playerctl("position") or 0)
        except ValueError:
            position = 0

        self._status = status
        self._length = length
        self._position = position

        self.seek_scale.set_sensitive(length > 0)
        if not self._seeking:
            self.seek_scale.set_range(0, max(length, 1))
            self.seek_scale.set_value(min(position, length) if length else 0)
        self.pos_label.set_text(self._fmt_time(position))
        self.dur_label.set_text(self._fmt_time(length))
        return True

    _YOUTUBE_ID_RE = re.compile(r"(?:v=|youtu\.be/|embed/|shorts/)([A-Za-z0-9_-]{11})")

    @classmethod
    def _youtube_thumbnail_url(cls, page_url):
        if not page_url:
            return ""
        match = cls._YOUTUBE_ID_RE.search(page_url)
        if not match:
            return ""
        return f"https://i.ytimg.com/vi/{match.group(1)}/hqdefault.jpg"

    def _set_art(self, url):
        pixbuf = None
        try:
            if not url:
                pass
            elif url.startswith("file://"):
                path = urllib.request.url2pathname(url[len("file://"):])
                raw = GdkPixbuf.Pixbuf.new_from_file(path)
                pixbuf = self._scale_cover(raw, ART_SIZE)
            elif url.startswith("http://") or url.startswith("https://"):
                # Some CDNs (e.g. YouTube's) reject requests without a
                # browser-like User-Agent, so spoof one.
                req = urllib.request.Request(
                    url, headers={"User-Agent": "Mozilla/5.0"}
                )
                with urllib.request.urlopen(req, timeout=4) as resp:
                    data = resp.read()
                loader = GdkPixbuf.PixbufLoader()
                loader.write(data)
                loader.close()
                pixbuf = self._scale_cover(loader.get_pixbuf(), ART_SIZE)
        except (GLib.Error, OSError):
            pixbuf = None

        if pixbuf:
            self.art.set_from_pixbuf(pixbuf)
        else:
            self.art.set_from_icon_name("audio-x-generic-symbolic", Gtk.IconSize.DIALOG)

    @staticmethod
    def _scale_cover(pixbuf, size):
        """Scale+crop to fill a size x size square without distorting aspect ratio."""
        w, h = pixbuf.get_width(), pixbuf.get_height()
        scale = max(size / w, size / h)
        new_w, new_h = max(round(w * scale), size), max(round(h * scale), size)
        scaled = pixbuf.scale_simple(new_w, new_h, GdkPixbuf.InterpType.BILINEAR)
        x, y = (new_w - size) // 2, (new_h - size) // 2
        return scaled.new_subpixbuf(x, y, size, size)


def main():
    pid = already_running()
    if pid:
        os.kill(pid, signal.SIGTERM)
        return

    provider = Gtk.CssProvider()
    provider.load_from_path(CSS_PATH)
    Gtk.StyleContext.add_provider_for_screen(
        Gdk.Screen.get_default(), provider, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION
    )

    with open(PIDFILE, "w") as f:
        f.write(str(os.getpid()))

    win = Popup()
    signal.signal(signal.SIGTERM, lambda *_: Gtk.main_quit())

    win.show_all()
    try:
        Gtk.main()
    finally:
        try:
            os.remove(PIDFILE)
        except FileNotFoundError:
            pass


if __name__ == "__main__":
    main()
