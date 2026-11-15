// playerctl calls and MPRIS/D-Bus lookups.

use std::process::{Command, Stdio};

use glib::ToVariant;

// playerctld proxies whichever player is currently active under one fixed bus name
const MPRIS_BUS_NAME: &str = "org.mpris.MediaPlayer2.playerctld";
pub(crate) const MPRIS_OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
pub(crate) const DBUS_PROPERTIES_IFACE: &str = "org.freedesktop.DBus.Properties";
pub(crate) const MPRIS_PLAYER_IFACE: &str = "org.mpris.MediaPlayer2.Player";
pub(crate) const MPRIS_BUS_PREFIX: &str = "org.mpris.MediaPlayer2.";

const PLAYERCTLD_IFACE: &str = "com.github.altdesktop.playerctld";

/// One line per player, fields split by ASCII unit separator.
const METADATA_FORMAT: &str = concat!(
    "{{playerInstance}}\u{1f}",
    "{{status}}\u{1f}",
    "{{title}}\u{1f}",
    "{{artist}}\u{1f}",
    "{{mpris:artUrl}}\u{1f}",
    "{{mpris:length}}\u{1f}",
    "{{position}}\u{1f}",
    "{{xesam:url}}",
);
const METADATA_SEP: char = '\u{1f}';

pub(crate) struct PlayerInfo {
    /// Full instance name ("firefox.instance_1_82"), what `-p` takes.
    pub(crate) instance: String,
    pub(crate) status: String,
    pub(crate) title: String,
    pub(crate) artist: String,
    pub(crate) art_url: String,
    pub(crate) page_url: String,
    // Both in seconds.
    pub(crate) length: f64,
    pub(crate) position: f64,
}

impl PlayerInfo {
    pub(crate) fn is_playing(&self) -> bool {
        self.status == "Playing"
    }

    fn parse(line: &str) -> Option<Self> {
        let mut fields = line.split(METADATA_SEP);
        let mut next = move || fields.next().unwrap_or_default().to_string();
        let instance = next();
        if instance.is_empty() || instance == "playerctld" {
            return None;
        }
        // mpris:length and {{position}} are in μs
        let secs = |s: String| s.parse::<f64>().unwrap_or_default() / 1_000_000.0;
        Some(Self {
            instance,
            status: next(),
            title: next(),
            artist: next(),
            art_url: next(),
            length: secs(next()),
            position: secs(next()),
            page_url: next(),
        })
    }
}

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

/// playerctl aimed at one player; no-op without one.
pub(crate) fn playerctl_for(player: &str, args: &[&str]) -> String {
    if player.is_empty() {
        return String::new();
    }
    let mut full = vec!["-p", player];
    full.extend_from_slice(args);
    playerctl(&full)
}

/// Every player in one playerctl call.
pub(crate) fn list_players() -> Vec<PlayerInfo> {
    // Exit status ignored: one player without metadata shouldn't hide the rest.
    let Ok(out) = Command::new("playerctl")
        .args(["-a", "metadata", "--format", METADATA_FORMAT])
        .stderr(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(PlayerInfo::parse)
        .collect()
}

/// Most recently active player, per playerctld's PlayerNames ordering.
pub(crate) fn active_player() -> Option<String> {
    let conn = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok()?;
    let reply = conn
        .call_sync(
            Some(MPRIS_BUS_NAME),
            MPRIS_OBJECT_PATH,
            DBUS_PROPERTIES_IFACE,
            "Get",
            Some(&(PLAYERCTLD_IFACE, "PlayerNames").to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            1000,
            gio::Cancellable::NONE,
        )
        .ok()?;
    let names: Vec<String> = reply.child_value(0).as_variant()?.get()?;
    names.first()?.strip_prefix(MPRIS_BUS_PREFIX).map(str::to_string)
}

/// Drop any instance suffix ("firefox.instance_1_88" -> "firefox").
pub(crate) fn short_name(instance: &str) -> &str {
    instance.split('.').next().unwrap_or(instance)
}

/// PID of the process behind a player, by asking the bus who owns its name.
pub(crate) fn player_pid(player_name: &str) -> Option<u32> {
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
    // Bus name may carry an instance suffix ("firefox.instance_1_96").
    let wanted = format!("{MPRIS_BUS_PREFIX}{player_name}");
    let instance_prefix = format!("{wanted}.");
    let bus_name = names
        .into_iter()
        .find(|n: &String| *n == wanted || n.starts_with(&instance_prefix))?;

    call("GetConnectionUnixProcessID", Some(&(bus_name,).to_variant()))?
        .child_value(0)
        .get()
}

/// Focus the player's window via its bus PID and Hyprland.
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
