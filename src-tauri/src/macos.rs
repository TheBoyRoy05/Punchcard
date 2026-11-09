use objc2::MainThreadMarker;
use objc2_app_kit::{NSEvent, NSScreen, NSWindow, NSWindowCollectionBehavior};
use objc2_foundation::NSPoint;
use tauri::WebviewWindow;

pub fn clicked_away() -> bool {
    NSEvent::pressedMouseButtons() != 0
}

pub fn place_popup(window: &WebviewWindow) -> tauri::Result<()> {
    let mtm = MainThreadMarker::new().expect("Tauri runs setup on the main thread");
    let ns_window = window.ns_window()?;
    // SAFETY: Tauri returns a live NSWindow pointer, and mtm proves this is the main thread AppKit requires.
    let ns_window = unsafe { &*ns_window.cast::<NSWindow>() };

    // Tauri's visibleOnAllWorkspaces sets CanJoinAllSpaces only; a full-screen app's Space also needs FullScreenAuxiliary.
    let behavior = ns_window.collectionBehavior() | NSWindowCollectionBehavior::FullScreenAuxiliary;
    ns_window.setCollectionBehavior(behavior);

    // Tauri's center uses the main screen; open on the cursor's screen instead, like choose.
    // Cocoa coordinates throughout, since tao mixes scale factors across Retina and non-Retina screens.
    let mouse = NSEvent::mouseLocation();
    let screens = NSScreen::screens(mtm);
    let cursor_screen = screens.iter().find(|screen| {
        let frame = screen.frame();
        (frame.origin.x..frame.origin.x + frame.size.width).contains(&mouse.x)
            && (frame.origin.y..frame.origin.y + frame.size.height).contains(&mouse.y)
    });
    let Some(screen) = cursor_screen else {
        eprintln!("stint: no screen holds the cursor at {mouse:?}, window stays centered");
        return Ok(());
    };

    let area = screen.visibleFrame();
    let size = ns_window.frame().size;
    let x = area.origin.x + (area.size.width - size.width) / 2.0;
    let y = area.origin.y + (area.size.height - size.height) / 2.0;
    ns_window.setFrameOrigin(NSPoint::new(x, y));
    Ok(())
}
