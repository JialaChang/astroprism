// Author: JialaChang

// Floating GTK popup for waybar's mpris module.
// State via playerctl, updates from D-Bus signals rather than polling.

mod art;
mod mpris;
mod ui;

use gtk::prelude::*;

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

/// Per-user runtime dir, not /tmp, so no one can plant a symlink there.
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
