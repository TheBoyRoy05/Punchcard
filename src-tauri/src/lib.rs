use std::path::Path;

#[cfg(target_os = "macos")]
mod macos;
pub mod note;
pub mod notify;
pub mod popup;
pub mod rank;
pub mod timeline;
pub mod vault;

pub fn toggle_and_notify(vault: &Path, note: &Path) -> Result<note::Note, vault::TimerError> {
    let (toggle, toggled) = vault::toggle(vault, note)?;
    let name = toggled.name();
    println!("punch: {toggle} {name}");
    notify::show(&toggle.to_string(), name, Some(&toggled.url()));
    Ok(toggled)
}
