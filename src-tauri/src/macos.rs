use objc2::MainThreadMarker;
use objc2_app_kit::{NSEvent, NSScreen, NSWindow, NSWindowCollectionBehavior};
use objc2_foundation::{NSPoint, NSRect, NSSize};
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
        eprintln!("punch: no screen holds the cursor at {mouse:?}, window stays centered");
        return Ok(());
    };

    let area = screen.visibleFrame();
    let size = ns_window.frame().size;
    let x = area.origin.x + (area.size.width - size.width) / 2.0;
    let y = area.origin.y + (area.size.height - size.height) / 2.0;
    ns_window.setFrameOrigin(NSPoint::new(x, y));
    Ok(())
}

// `height` maps the screen's visible height to the content height. The top edge stays put, since `setContentSize`
// keeps the bottom-left origin and would move the header on every resize.
pub fn fit(window: &WebviewWindow, height: impl FnOnce(f64) -> f64) -> tauri::Result<()> {
    MainThreadMarker::new().expect("the caller runs this on the main thread");
    let ns_window = window.ns_window()?;
    // SAFETY: Tauri returns a live NSWindow pointer, and the marker above proves this is the main thread AppKit requires.
    let ns_window = unsafe { &*ns_window.cast::<NSWindow>() };
    let Some(screen) = ns_window.screen() else {
        eprintln!("punch: the timeline is on no screen, so it keeps its size");
        return Ok(());
    };

    let visible = screen.visibleFrame();
    let frame = ns_window.frame();
    let mut content = ns_window.contentRectForFrameRect(frame);
    content.size.height = height(visible.size.height);
    let resized = ns_window.frameRectForContentRect(content).size.height;
    ns_window.setFrame_display(top_kept(frame, resized, visible), true);
    Ok(())
}

// Moves the window up only as far as its bottom needs to stay inside the visible frame.
fn top_kept(frame: NSRect, height: f64, visible: NSRect) -> NSRect {
    let top = frame.origin.y + frame.size.height;
    let y = (top - height).max(visible.origin.y);
    NSRect::new(
        NSPoint::new(frame.origin.x, y),
        NSSize::new(frame.size.width, height),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(y: f64, height: f64) -> NSRect {
        NSRect::new(NSPoint::new(10.0, y), NSSize::new(800.0, height))
    }

    #[test]
    fn a_resize_keeps_the_top_edge() {
        assert_eq!(
            top_kept(rect(400.0, 600.0), 300.0, rect(0.0, 1400.0)),
            rect(700.0, 300.0)
        );
    }

    #[test]
    fn a_resize_moves_the_window_up_only_to_keep_its_bottom_on_the_screen() {
        assert_eq!(
            top_kept(rect(400.0, 600.0), 1100.0, rect(25.0, 1400.0)),
            rect(25.0, 1100.0)
        );
    }
}
