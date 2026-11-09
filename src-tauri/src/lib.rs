use std::path::Path;

#[cfg(target_os = "macos")]
mod macos;
pub mod note;
pub mod notify;
pub mod picker;
pub mod popup;
pub mod rank;
pub mod timeline;
pub mod vault;

pub fn toggle_and_notify(vault: &Path, name: &str) -> Result<(), vault::TimerError> {
    let (toggle, toggled) = vault::toggle(vault, name)?;
    println!("stint: {toggle} {name}");
    notify::show(&toggle.to_string(), name, Some(&toggled.url()));
    Ok(())
}

// The picker and the timeline share one copy of the embedded pages.
fn context() -> tauri::Context {
    tauri::generate_context!()
}
