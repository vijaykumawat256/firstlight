//! Containing panics. With unwinding panics, a bug triggered by untrusted data can be confined to the task
//! that hit it instead of ending the app. Background loops run their work through [`contained`].

use std::panic::{AssertUnwindSafe, catch_unwind};

/// Runs `f`, returning `None` if it panicked. The panic message is not inspected or logged here: it could
/// contain content from the data being processed.
pub fn contained<T>(f: impl FnOnce() -> T) -> Option<T> {
    catch_unwind(AssertUnwindSafe(f)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_is_contained_and_the_caller_carries_on() {
        assert_eq!(contained(|| 7), Some(7));
        let out: Option<i32> = contained(|| panic!("boom: private event title"));
        assert_eq!(out, None);
        // The thread is still usable afterwards.
        assert_eq!(contained(|| "still alive"), Some("still alive"));
    }

    #[test]
    fn repeated_panics_do_not_accumulate_damage() {
        for i in 0..50 {
            assert_eq!(
                contained(|| if i % 2 == 0 { panic!("x") } else { i }),
                if i % 2 == 0 { None } else { Some(i) }
            );
        }
    }
}
