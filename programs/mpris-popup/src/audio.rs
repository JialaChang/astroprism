// Per-app volume: the player's PipeWire streams, via pactl.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

use serde_json::Value;

use crate::mpris;

/// pactl's 100% in raw volume units.
const VOLUME_NORM: f64 = 65536.0;
/// 2.0 = 200%; above 1.0 is software boost.
pub(crate) const MAX_VOLUME: f64 = 2.0;

pub(crate) struct Stream {
    index: u64,
    /// 1.0 = 100%, averaged over channels
    pub(crate) volume: f64,
    pub(crate) muted: bool,
}

fn pactl(args: &[&str]) -> Option<Vec<u8>> {
    Command::new("pactl")
        .args(args)
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| o.stdout)
}

fn parse_stream(v: &Value) -> Option<Stream> {
    let channels = v["volume"].as_object()?;
    let sum: f64 = channels.values().filter_map(|c| c["value"].as_f64()).sum();
    Some(Stream {
        index: v["index"].as_u64()?,
        volume: sum / channels.len().max(1) as f64 / VOLUME_NORM,
        muted: v["mute"].as_bool().unwrap_or(false),
    })
}

/// The player's streams: by bus PID, else by app name (Spotify sets no PID).
pub(crate) fn streams_for(instance: &str) -> Vec<Stream> {
    let Some(out) = pactl(&["-f", "json", "list", "sink-inputs"]) else {
        return Vec::new();
    };
    let Ok(Value::Array(inputs)) = serde_json::from_slice::<Value>(&out) else {
        return Vec::new();
    };

    let prop = |v: &Value, key: &str| v["properties"][key].as_str().map(str::to_string);
    let collect = |keep: &dyn Fn(&Value) -> bool| -> Vec<Stream> {
        inputs.iter().filter(|v| keep(v)).filter_map(parse_stream).collect()
    };

    if let Some(pid) = mpris::player_pid(instance).map(|p| p.to_string()) {
        let by_pid = collect(&|v| prop(v, "application.process.id").as_deref() == Some(&pid));
        if !by_pid.is_empty() {
            return by_pid;
        }
    }
    let name = mpris::short_name(instance);
    collect(&|v| prop(v, "application.name").is_some_and(|n| n.eq_ignore_ascii_case(name)))
}

pub(crate) fn set_volume(streams: &[Stream], volume: f64) {
    let pct = format!("{}%", (volume.clamp(0.0, MAX_VOLUME) * 100.0).round());
    for s in streams {
        pactl(&["set-sink-input-volume", &s.index.to_string(), &pct]);
    }
}

pub(crate) fn toggle_mute(streams: &[Stream]) {
    for s in streams {
        pactl(&["set-sink-input-mute", &s.index.to_string(), "toggle"]);
    }
}

/// Watch `pactl subscribe` and ping `tx` on sink-input events; caller kills the child.
pub(crate) fn spawn_subscribe(tx: glib::Sender<()>) -> Option<Child> {
    let mut child = Command::new("pactl")
        .arg("subscribe")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line.contains("sink-input") && tx.send(()).is_err() {
                break;
            }
        }
    });
    Some(child)
}
