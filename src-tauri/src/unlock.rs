//! Screen-unlock detection. Calls `on_unlock` each time the user unlocks the computer.
//!
//! macOS: observes the distributed notification `com.apple.screenIsUnlocked`.
//! Windows: session-change notifications. Other platforms: not available.

#[cfg(target_os = "macos")]
pub fn start(on_unlock: impl Fn() + Send + Sync + 'static) {
    use block2::RcBlock;
    use objc2_foundation::{NSDistributedNotificationCenter, NSNotification, NSString};
    use std::ptr::NonNull;

    let center = NSDistributedNotificationCenter::defaultCenter();
    let name = NSString::from_str("com.apple.screenIsUnlocked");
    let block = RcBlock::new(move |_n: NonNull<NSNotification>| on_unlock());
    // SAFETY: the block is `'static` and only calls the Send + Sync closure.
    let token = unsafe {
        center.addObserverForName_object_queue_usingBlock(Some(&name), None, None, &block)
    };
    // The observer must live for the whole process; dropping the token would not remove it,
    // but we keep it alive explicitly for clarity.
    std::mem::forget(token);
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
