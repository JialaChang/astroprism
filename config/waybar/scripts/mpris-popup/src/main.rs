// Author: JialaChang

// Floating GTK popup for waybar's mpris module.
//
// Shows the player name, cover art, title/artist, a seek bar and prev/play-pause/next
// controls. Clicking the player name focuses that app's own window.
//
// State comes from playerctl; playback commands go back out the same way.
// Updates are signal-driven rather than polled: PropertiesChanged from playerctld,
// plus Seeked for the position MPRIS never notifies on. The only timers are the
// seek bar's per-second advance and the idle timeout that closes the popup.
// Remote cover art is fetched off the main thread.

mod art;
mod mpris;
mod ui;

use gtk::prelude::*;

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

/// Per-user runtime dir rather than shared `/tmp`,
/// so another local user can't plant a symlink at a predictable path.
fn pidfile_path() -> String {
    std::env::var("XDG_RUNTIME_DIR")
        .map(|dir| format!("{dir}/waybar-mpris-popup.pid"))
        .unwrap_or_else(|_| "/tmp/waybar-mpris-popup.pid".to_string())
}

fn already_running() -> Option<i32> {
    let content = std::fs::read_to_string(pidfile_path()).ok()?;
    let pid: i32 = content.trim().parse().ok()?;
    use nix::sys::signal::kill;
    use nix::unistd::Pid;
    kill(Pid::from_raw(pid), None).ok()?;
    Some(pid)
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

    let _ = std::fs::write(pidfile_path(), std::process::id().to_string());

    glib::source::unix_signal_add(SIGTERM, || {
        gtk::main_quit();
        glib::ControlFlow::Break
    });

    let window = ui::build_window();
    window.show_all();
    gtk::main();

    let _ = std::fs::remove_file(pidfile_path());
}
