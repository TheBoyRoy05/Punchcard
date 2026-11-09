use std::path::PathBuf;

use chrono::Local;
use punchcard_core::time::wall_clock;
use serde::Serialize;
use tauri::{State, WebviewWindowBuilder, WindowEvent};

#[cfg(target_os = "macos")]
use crate::macos;
use crate::vault::{self, TimerError};
use crate::{popup, rank, toggle_and_notify};

const WINDOW: &str = "picker";

struct Vault(PathBuf);

#[derive(Serialize)]
struct Task {
    name: String,
    running: bool,
}

pub fn open(vault: PathBuf) -> Result<(), TimerError> {
    tauri::Builder::default()
        .manage(Vault(vault))
        .invoke_handler(tauri::generate_handler![tasks, toggle])
        .on_window_event(|window, event| {
            if let WindowEvent::Focused(false) = event {
                popup::on_blur(window);
            }
        })
        .setup(|app| {
            // An accessory app has no Dock icon and may draw over another app's full-screen Space, like choose.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let windows = &app.config().app.windows;
            let config = windows.iter().find(|window| window.label == WINDOW);
            let config = config.ok_or_else(|| format!("tauri.conf.json has no {WINDOW} window"))?;
            #[cfg_attr(not(target_os = "macos"), expect(unused_variables))]
            let window = WebviewWindowBuilder::from_config(app.handle(), config)?.build()?;

            #[cfg(target_os = "macos")]
            macos::place_popup(&window)?;
            Ok(())
        })
        .run(crate::context())
        .map_err(|error| format!("picker failed: {error}").into())
}

#[tauri::command(async)]
fn tasks(vault: State<'_, Vault>) -> Result<Vec<Task>, TimerError> {
    let notes = vault::notes(&vault.0)?;
    let ranked = rank::ranked(notes, wall_clock(Local::now().naive_local()));
    let tasks = ranked.iter().map(|note| Task {
        name: note.name().to_string(),
        running: note.log.running(),
    });
    Ok(tasks.collect())
}

#[tauri::command(async)]
fn toggle(vault: State<'_, Vault>, name: String) -> Result<(), TimerError> {
    toggle_and_notify(&vault.0, &name)
}
