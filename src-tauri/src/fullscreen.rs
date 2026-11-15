//! Is another app covering the whole screen (a presentation, a film, a game)? When it is, an automatic
//! appearance would land on top of it, so the unlock rules stay out of the way (see `skip_when_fullscreen`).
//!
//! macOS: a full-screen window is a normal window (layer 0) whose frame is exactly a display's frame,
//! menu bar included. A maximised window stops below the menu bar, so it never matches. Reading window
//! frames needs no permission (window titles would). Windows: the shell's own "do not disturb" state.

/// A window as the window server describes it, in screen points.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub struct Win {
    pub owner_pid: i64,
    pub layer: i64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// One display's frame: `(x, y, width, height)`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub type Frame = (f64, f64, f64, f64);

/// Whether some window that is not ours exactly covers one of the displays.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn covers_a_display(windows: &[Win], displays: &[Frame], own_pid: i64) -> bool {
    const TOLERANCE: f64 = 1.5; // frames are reported in points and may be off by a rounding
    windows.iter().any(|win| {
        win.owner_pid != own_pid
            && win.layer == 0
            && displays.iter().any(|&(x, y, w, h)| {
                (win.x - x).abs() <= TOLERANCE
                    && (win.y - y).abs() <= TOLERANCE
                    && (win.w - w).abs() <= TOLERANCE
                    && (win.h - h).abs() <= TOLERANCE
            })
    })
}

#[cfg(target_os = "macos")]
pub fn another_app_is_full_screen() -> bool {
    let (windows, displays) = snapshot();
    covers_a_display(&windows, &displays, i64::from(std::process::id()))
}

/// The on-screen windows and the displays, as the window server reports them right now.
#[cfg(target_os = "macos")]
fn snapshot() -> (Vec<Win>, Vec<Frame>) {
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use core_graphics::display::CGDisplay;
    use core_graphics::window::{
        copy_window_info, kCGWindowBounds, kCGWindowLayer, kCGWindowListExcludeDesktopElements,
        kCGWindowListOptionOnScreenOnly, kCGWindowOwnerPID,
    };

    let Some(list) = copy_window_info(
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
        0,
    ) else {
        return (vec![], vec![]);
    };
    // SAFETY: the keys are constants owned by CoreGraphics; `wrap_under_get_rule` retains them.
    let (layer_key, pid_key, bounds_key) = unsafe {
        (
            CFString::wrap_under_get_rule(kCGWindowLayer),
            CFString::wrap_under_get_rule(kCGWindowOwnerPID),
            CFString::wrap_under_get_rule(kCGWindowBounds),
        )
    };
    let number = |d: &CFDictionary<CFString, CFType>, key: &CFString| -> Option<f64> {
        d.find(key)?.downcast::<CFNumber>()?.to_f64()
    };

    let mut windows = Vec::new();
    for item in list.iter() {
        // SAFETY: every element of this array is a CFDictionary (documented for CGWindowListCopyWindowInfo).
        let dict: CFDictionary<CFString, CFType> =
            unsafe { CFDictionary::wrap_under_get_rule(*item as CFDictionaryRef) };
        let (Some(layer), Some(pid)) = (number(&dict, &layer_key), number(&dict, &pid_key)) else {
            continue;
        };
        let Some(bounds) = dict
            .find(&bounds_key)
            .and_then(|b| b.downcast::<CFDictionary>())
        else {
            continue;
        };
        // SAFETY: the bounds dictionary maps CFString keys to CFNumber values (documented).
        let bounds: CFDictionary<CFString, CFType> =
            unsafe { CFDictionary::wrap_under_get_rule(bounds.as_concrete_TypeRef()) };
        let get = |k: &str| number(&bounds, &CFString::new(k));
        let (Some(x), Some(y), Some(w), Some(h)) =
            (get("X"), get("Y"), get("Width"), get("Height"))
        else {
            continue;
        };
        windows.push(Win {
            owner_pid: pid as i64,
            layer: layer as i64,
            x,
            y,
            w,
            h,
        });
    }

    let displays: Vec<Frame> = CGDisplay::active_displays()
        .unwrap_or_default()
        .into_iter()
        .map(|id| {
            let b = CGDisplay::new(id).bounds();
            (b.origin.x, b.origin.y, b.size.width, b.size.height)
        })
        .collect();
    (windows, displays)
}

/// Windows: `SHQueryUserNotificationState` reports a full-screen app, a game, or presentation mode.
/// Written against the documented API; never compiled or run on Windows by us (CI builds it first).
#[cfg(windows)]
pub fn another_app_is_full_screen() -> bool {
    use windows::Win32::UI::Shell::{
        QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
        SHQueryUserNotificationState,
    };
    // SAFETY: a plain query with no arguments.
    match unsafe { SHQueryUserNotificationState() } {
        Ok(s) => s == QUNS_BUSY || s == QUNS_PRESENTATION_MODE || s == QUNS_RUNNING_D3D_FULL_SCREEN,
        Err(_) => false,
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn another_app_is_full_screen() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Frame = (0.0, 0.0, 1512.0, 982.0);
    const SECOND: Frame = (1512.0, 0.0, 1920.0, 1080.0);
    const OURS: i64 = 100;

    fn win(pid: i64, layer: i64, (x, y, w, h): Frame) -> Win {
        Win {
            owner_pid: pid,
            layer,
            x,
            y,
            w,
            h,
        }
    }

    #[test]
    fn a_window_exactly_covering_a_display_is_full_screen() {
        let video = win(7, 0, SCREEN);
        assert!(covers_a_display(&[video], &[SCREEN], OURS));
        // Off by a rounding.
        assert!(covers_a_display(
            &[win(7, 0, (0.0, 0.5, 1512.0, 981.0))],
            &[SCREEN],
            OURS
        ));
    }

    #[test]
    fn a_maximised_window_is_not_full_screen() {
        // Stops below the 38-point menu bar.
        assert!(!covers_a_display(
            &[win(7, 0, (0.0, 38.0, 1512.0, 944.0))],
            &[SCREEN],
            OURS
        ));
        assert!(!covers_a_display(
            &[win(7, 0, (100.0, 100.0, 800.0, 600.0))],
            &[SCREEN],
            OURS
        ));
    }

    #[test]
    fn our_own_windows_and_system_layers_do_not_count() {
        assert!(!covers_a_display(&[win(OURS, 0, SCREEN)], &[SCREEN], OURS));
        // The menu bar, the Dock and overlays live on other layers.
        assert!(!covers_a_display(
            &[
                win(7, 24, SCREEN),
                win(7, 25, SCREEN),
                win(7, -2_147_483_623, SCREEN)
            ],
            &[SCREEN],
            OURS
        ));
    }

    #[test]
    fn works_on_a_second_display() {
        assert!(covers_a_display(
            &[win(7, 0, SECOND)],
            &[SCREEN, SECOND],
            OURS
        ));
        assert!(!covers_a_display(&[win(7, 0, SECOND)], &[SCREEN], OURS));
        assert!(!covers_a_display(&[], &[SCREEN, SECOND], OURS));
        assert!(!covers_a_display(&[win(7, 0, SCREEN)], &[], OURS));
    }

    /// Live probe on a Mac: reads the real window list and checks it is not empty (an unreadable list would
    /// otherwise look the same as "nothing is full screen"). Run with
    /// `cargo test -p firstlight fullscreen -- --ignored --nocapture`.
    #[test]
    #[ignore]
    #[cfg(target_os = "macos")]
    fn live_probe() {
        let (windows, displays) = snapshot();
        println!("displays: {displays:?}");
        println!("on-screen windows read: {}", windows.len());
        for w in windows.iter().filter(|w| w.layer == 0).take(5) {
            println!(
                "  normal window: pid {} at ({}, {}) size {} x {}",
                w.owner_pid, w.x, w.y, w.w, w.h
            );
        }
        println!(
            "another app is full screen: {}",
            another_app_is_full_screen()
        );
        assert!(!displays.is_empty(), "no displays read");
        assert!(
            !windows.is_empty(),
            "no windows read: the window list could not be parsed"
        );
    }
}
