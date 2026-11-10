use std::env;
use std::process::{Command, Stdio};

use notify_rust::{Notification, NotificationResponse};

// punch has no app bundle, so it posts as an app that has one. On macOS 26 only Terminal's posts showed;
// Obsidian's and terminal-notifier's were dropped silently.
#[cfg(target_os = "macos")]
const POST_AS: &str = "com.apple.Terminal";
// The action key XDG reports for a body click. macOS and Windows report a body click as Default instead.
const OPEN_ACTION: &str = "default";

// Acting on a click means waiting for it, which outlives a toggle or a timeline window, so a detached `punch notify` waits instead.
pub fn show(title: &str, message: &str, url: Option<&str>) {
    let Some(url) = url else {
        if let Err(error) = notification(title, message).show() {
            eprintln!("punch: notification not shown: {error}");
        }
        return;
    };

    // `--` keeps a note named like a flag from reaching clap as one. With no stdio and its own process group,
    // the child neither holds a caller's output pipe open nor dies with a Ctrl-C in the caller's terminal.
    let spawned = env::current_exe().and_then(|punch| {
        let mut command = Command::new(punch);
        command
            .args(["notify", "--", title, message, url])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        command.spawn()
    });
    if let Err(error) = spawned {
        eprintln!("punch: notification not shown: {error}");
    }
}

// Returns once the notification is clicked, dismissed or, on macOS, gone from the screen.
pub fn show_and_open_on_click(title: &str, message: &str, url: &str) -> Result<(), String> {
    let shown = notification(title, message)
        .action(OPEN_ACTION, "Open note")
        .show()
        .map_err(|error| format!("notification not shown: {error}"))?;

    let mut clicked = false;
    shown
        .wait_for_response(|response: &NotificationResponse| {
            clicked = match response {
                NotificationResponse::Default => true,
                NotificationResponse::Action(action) => action == OPEN_ACTION,
                _ => false,
            };
        })
        .map_err(|error| format!("notification not shown: {error}"))?;

    if clicked {
        open::that(url).map_err(|error| format!("{url} not opened: {error}"))?;
    }
    Ok(())
}

fn notification(title: &str, message: &str) -> Notification {
    #[cfg(target_os = "macos")]
    if let Err(error) = notify_rust::set_application(POST_AS) {
        eprintln!("punch: notifications not posted as {POST_AS}: {error}");
    }

    let mut notification = Notification::new();
    notification.summary(title).body(message);
    notification
}
