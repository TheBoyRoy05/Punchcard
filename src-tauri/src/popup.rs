use tauri::Window;

#[cfg(target_os = "macos")]
use crate::macos::clicked_away;

// Closes on a click elsewhere, like choose. Focus can also leave without a click, on a Space
// switch or when an app such as Arc grabs focus as the cursor passes over it; those refocus instead.
pub fn on_blur(window: &Window) {
    let (action, result) = if clicked_away() {
        ("closed", window.close())
    } else {
        ("refocused", window.set_focus())
    };
    if let Err(error) = result {
        eprintln!("stint: {} not {action}: {error}", window.label());
    }
}

#[cfg(not(target_os = "macos"))]
fn clicked_away() -> bool {
    true
}
