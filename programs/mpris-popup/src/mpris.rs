// playerctl calls and MPRIS/D-Bus lookups.

use std::process::{Command, Stdio};

use glib::ToVariant;

// playerctld proxies whichever player is currently active under one fixed bus name
pub(crate) const MPRIS_BUS_NAME: &str = "org.mpris.MediaPlayer2.playerctld";
pub(crate) const MPRIS_OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
pub(crate) const DBUS_PROPERTIES_IFACE: &str = "org.freedesktop.DBus.Properties";
pub(crate) const MPRIS_PLAYER_IFACE: &str = "org.mpris.MediaPlayer2.Player";
const MPRIS_BUS_PREFIX: &str = "org.mpris.MediaPlayer2.";

/// Everything a refresh needs, in one playerctl call instead of one per field.
/// Fields are separated by an ASCII unit separator,
/// which playerctl passes through and metadata never contains.
pub(crate) const METADATA_FORMAT: &str = concat!(
    "{{playerName}}\u{1f}",
    "{{title}}\u{1f}",
    "{{artist}}\u{1f}",
    "{{mpris:artUrl}}\u{1f}",
    "{{mpris:length}}\u{1f}",
    "{{position}}\u{1f}",
    "{{xesam:url}}",
);
pub(crate) const METADATA_SEP: char = '\u{1f}';

pub(crate) fn playerctl(args: &[&str]) -> String {
    Command::new("playerctl")
        .args(args)
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
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
        .find(|n: &String| *n == wanted || n.starts_with(&instance_prefix))?;

    call("GetConnectionUnixProcessID", Some(&(bus_name,).to_variant()))?
        .child_value(0)
        .get()
}

/// Focus the window belonging to the given MPRIS player, matched by the bus
/// name's process ID and dispatched through Hyprland.
pub(crate) fn focus_player_window(player_name: &str) -> bool {
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
pub(crate) fn player_icon(player_name: &str) -> &'static str {
    let name = player_name.to_ascii_lowercase();
    if name.starts_with("spotify") {
        "\u{f1bc}"
    } else if name.starts_with("firefox") {
        "\u{e745}"
    } else {
        "\u{f144}"
    }
}
