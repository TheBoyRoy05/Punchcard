use std::process::Command;

// Native notifications need an app bundle with notification permission, which stint does not have yet.
const NOTIFIER: &str = "/opt/homebrew/bin/terminal-notifier";

pub fn show(title: &str, message: &str, url: Option<&str>) {
    let mut command = Command::new(NOTIFIER);
    command.args(["-title", title, "-message", message]);
    if let Some(url) = url {
        command.args(["-open", url]);
    }

    match command.status() {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!("stint: notification not shown: {NOTIFIER} {status}"),
        Err(error) => eprintln!("stint: notification not shown: {error}"),
    }
}
