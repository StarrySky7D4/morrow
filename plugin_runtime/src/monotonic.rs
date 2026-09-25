//! The browser host uses performance.now(), never wall-clock time, for deadlines.
#[cfg(not(target_arch = "wasm32"))]
pub use std::time::Instant;

#[cfg(target_arch = "wasm32")]
pub use browser::Instant;
#[cfg(target_arch = "wasm32")]
mod browser {
    use std::time::Duration;
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub struct Instant(Duration);
    #[cfg(feature = "web-packages")]
    #[wasm_bindgen::prelude::wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = performance, js_name = now)]
        fn now_ms() -> f64;
    }
    impl Instant {
        pub fn now() -> Self {
            #[cfg(feature = "web-packages")]
            let elapsed = Duration::try_from_secs_f64(now_ms() / 1000.0).unwrap_or(Duration::MAX);
            // No guessed clock for standalone Wasm runners; any explicit deadline
            // fails closed unless the browser clock adapter was selected.
            #[cfg(not(feature = "web-packages"))]
            let elapsed = Duration::MAX;
            Self(elapsed)
        }
        pub fn checked_add(self, duration: Duration) -> Option<Self> {
            self.0.checked_add(duration).map(Self)
        }
        pub fn elapsed(self) -> Duration {
            Self::now().0.saturating_sub(self.0)
        }
        pub fn checked_duration_since(self, earlier: Self) -> Option<Duration> {
            self.0.checked_sub(earlier.0)
        }
    }
}
