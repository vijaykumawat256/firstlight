//! Screen-unlock detection. Calls `on_unlock` each time the user unlocks the computer.
//!
//! macOS: observes the distributed notification `com.apple.screenIsUnlocked`.
//! Windows: session-change notifications. Other platforms: not available.

/// After waking from sleep we wait this long for the real unlock notification before acting on the wake alone.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const WAKE_GRACE: std::time::Duration = std::time::Duration::from_secs(20);

/// Whether the wake should count as an unlock: only if no real unlock was seen since the Mac woke. A Mac with
/// no password after sleep never sends one; a locked Mac sends it when the user signs in, and then the
/// wake must not count twice.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn wake_counts_as_unlock(
    last_unlock: Option<std::time::Instant>,
    woke_at: std::time::Instant,
) -> bool {
    last_unlock.is_none_or(|t| t < woke_at)
}

/// macOS: `com.apple.screenIsUnlocked` is the real signal. As a fallback (PLAN.md section 6), waking from
/// sleep counts as an unlock if no real unlock notification follows within [`WAKE_GRACE`]. The rules in
/// `briefing::rules` (minimum interval, first-unlock-of-the-day) make a repeat harmless.
#[cfg(target_os = "macos")]
pub fn start(on_unlock: impl Fn() + Send + Sync + 'static) {
    use block2::RcBlock;
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSDistributedNotificationCenter, NSNotification, NSString};
    use std::ptr::NonNull;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    let on_unlock = Arc::new(on_unlock);
    let last_unlock: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));

    let center = NSDistributedNotificationCenter::defaultCenter();
    let name = NSString::from_str("com.apple.screenIsUnlocked");
    let (cb, seen) = (on_unlock.clone(), last_unlock.clone());
    let block = RcBlock::new(move |_n: NonNull<NSNotification>| {
        if let Ok(mut t) = seen.lock() {
            *t = Some(Instant::now());
        }
        cb()
    });
    // SAFETY: the block is `'static` and only calls the Send + Sync closure.
    let token = unsafe {
        center.addObserverForName_object_queue_usingBlock(Some(&name), None, None, &block)
    };
    // The observer must live for the whole process; dropping the token would not remove it,
    // but we keep it alive explicitly for clarity.
    std::mem::forget(token);

    // Wake from sleep, announced by the workspace's own notification centre.
    let workspace_center = NSWorkspace::sharedWorkspace().notificationCenter();
    let wake_name = NSString::from_str("NSWorkspaceDidWakeNotification");
    let wake_block = RcBlock::new(move |_n: NonNull<NSNotification>| {
        let woke_at = Instant::now();
        let (cb, seen) = (on_unlock.clone(), last_unlock.clone());
        std::thread::spawn(move || {
            std::thread::sleep(WAKE_GRACE);
            let last = seen.lock().ok().and_then(|t| *t);
            if wake_counts_as_unlock(last, woke_at) {
                log::info!(
                    "woke from sleep with no unlock notification: treating the wake as an unlock"
                );
                cb();
            }
        });
    });
    // SAFETY: as above; the block only spawns a thread.
    let wake_token = unsafe {
        workspace_center.addObserverForName_object_queue_usingBlock(
            Some(&wake_name),
            None,
            None,
            &wake_block,
        )
    };
    std::mem::forget(wake_token);
}

/// Windows: a hidden message-only window registered for session notifications (`WTSRegisterSessionNotification`)
/// receives `WM_WTSSESSION_CHANGE` with `WTS_SESSION_UNLOCK` each time the user unlocks the workstation.
///
/// NOTE: written against the documented Win32 API but not compiled or run on the machine this was developed on;
/// the Windows job in CI (`.github/workflows/ci.yml`) is where it is first built. It is isolated behind
/// `cfg(windows)`, so it cannot affect other platforms.
#[cfg(windows)]
pub fn start(on_unlock: impl Fn() + Send + Sync + 'static) {
    use std::sync::OnceLock;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::RemoteDesktop::{
        NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, HWND_MESSAGE, MSG,
        RegisterClassW, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSW,
    };
    use windows::core::w;

    const WM_WTSSESSION_CHANGE: u32 = 0x02B1;
    const WTS_SESSION_UNLOCK: usize = 0x8;
    static CALLBACK: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_WTSSESSION_CHANGE && wparam.0 == WTS_SESSION_UNLOCK {
            if let Some(callback) = CALLBACK.get() {
                callback();
            }
            return LRESULT(0);
        }
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    if CALLBACK.set(Box::new(on_unlock)).is_err() {
        return; // already started
    }
    let spawned = std::thread::Builder::new()
        .name("unlock-watch".into())
        .spawn(|| unsafe {
            let Ok(instance) = GetModuleHandleW(None) else {
                log::error!("unlock detection: no module handle");
                return;
            };
            let class = w!("FirstlightUnlockWatcher");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance.into(),
                lpszClassName: class,
                ..Default::default()
            };
            if RegisterClassW(&wc) == 0 {
                log::error!("unlock detection: could not register the window class");
                return;
            }
            let hwnd = match CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                w!("Firstlight"),
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(instance.into()),
                None,
            ) {
                Ok(h) => h,
                Err(_) => {
                    log::error!("unlock detection: could not create the message window");
                    return;
                }
            };
            if WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION).is_err() {
                log::error!("unlock detection: could not register for session notifications");
                return;
            }
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        });
    if spawned.is_err() {
        log::error!("unlock detection: could not start its thread");
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn start(_on_unlock: impl Fn() + Send + Sync + 'static) {
    log::warn!("unlock detection is not available on this platform");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn a_wake_with_no_unlock_counts_as_one() {
        let woke = Instant::now();
        assert!(wake_counts_as_unlock(None, woke));
        // An unlock from before the sleep says nothing about this wake.
        assert!(wake_counts_as_unlock(
            woke.checked_sub(Duration::from_secs(3600)),
            woke
        ));
    }

    #[test]
    fn a_real_unlock_after_the_wake_means_the_wake_does_not_count_twice() {
        let woke = Instant::now();
        assert!(!wake_counts_as_unlock(
            Some(woke + Duration::from_secs(5)),
            woke
        ));
        // An unlock at the very moment of waking is the real one.
        assert!(!wake_counts_as_unlock(Some(woke), woke));
    }
}
