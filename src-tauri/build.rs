// Declaring the app's commands puts each one behind an allow- permission, so a window can call only the commands its capability grants.
const COMMANDS: &[&str] = &["load", "save", "toggle", "fit"];

fn main() {
    let manifest = tauri_build::AppManifest::new().commands(COMMANDS);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("tauri-build failed");
}
