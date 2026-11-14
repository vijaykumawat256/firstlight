//! Screen-unlock detection. Calls `on_unlock` each time the user unlocks the computer.
//!
//! macOS: observes the distributed notification `com.apple.screenIsUnlocked`.
//! Other platforms: not implemented in Phase 1 (Windows lands in Phase 6, behind this same function).

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

#[cfg(not(target_os = "macos"))]
pub fn start(_on_unlock: impl Fn() + Send + Sync + 'static) {
    log::warn!("unlock detection is not implemented on this platform yet");
}
