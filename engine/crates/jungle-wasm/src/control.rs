use std::sync::atomic::{AtomicU32, Ordering};

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use jungle_search::{SearchControl, SearchProgress};

#[cfg(target_arch = "wasm32")]
use js_sys::Function;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::wasm_bindgen;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = performance, js_name = now)]
    fn performance_now() -> f64;
}

/// Search boundary used by both the authoritative and detached worker paths.
///
/// A disposable search Worker is the reliable hard-cancellation mechanism in
/// browsers without shared Wasm memory. The atomic generation remains useful
/// for native tests, threaded builds, and browsers whose glue exposes the Wasm
/// memory to an `Atomics.store` caller.
pub(crate) struct BridgeSearchControl<'a> {
    cancel_search_id: &'a AtomicU32,
    search_id: u32,
    lane_id: u32,
    base_revision: u64,
    position_hash: &'a str,
    #[cfg(target_arch = "wasm32")]
    progress_callback: Option<&'a Function>,
    #[cfg(not(target_arch = "wasm32"))]
    started: Instant,
}

impl<'a> BridgeSearchControl<'a> {
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn new(
        cancel_search_id: &'a AtomicU32,
        search_id: u32,
        lane_id: u32,
        base_revision: u64,
        position_hash: &'a str,
        progress_callback: Option<&'a Function>,
    ) -> Self {
        Self {
            cancel_search_id,
            search_id,
            lane_id,
            base_revision,
            position_hash,
            progress_callback,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn new(
        cancel_search_id: &'a AtomicU32,
        search_id: u32,
        lane_id: u32,
        base_revision: u64,
        position_hash: &'a str,
    ) -> Self {
        Self {
            cancel_search_id,
            search_id,
            lane_id,
            base_revision,
            position_hash,
            started: Instant::now(),
        }
    }
}

impl SearchControl for BridgeSearchControl<'_> {
    #[inline]
    fn now_ms(&self) -> f64 {
        #[cfg(target_arch = "wasm32")]
        {
            performance_now()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.started.elapsed().as_secs_f64() * 1_000.0
        }
    }

    #[inline]
    fn should_stop(&self) -> bool {
        self.cancel_search_id.load(Ordering::Acquire) == self.search_id
    }

    fn on_iteration(&self, progress: &SearchProgress) {
        #[cfg(target_arch = "wasm32")]
        if let Some(callback) = self.progress_callback {
            let message = serde_json::json!({
                "searchId": self.search_id,
                "laneId": self.lane_id,
                "baseRevision": self.base_revision,
                "positionHash": self.position_hash,
                "depth": progress.depth,
                "selectiveDepth": progress.selective_depth,
                "nodes": progress.nodes,
                "elapsedMs": progress.elapsed_ms,
                "score": progress.score,
                "bestMove": progress.best_move,
                "principalVariation": progress.principal_variation,
            });
            let serialized = serde_json::to_string(&message)
                .expect("search progress contains only serializable values");
            if callback
                .call1(&JsValue::NULL, &JsValue::from_str(&serialized))
                .is_err()
            {
                // A throwing/detached callback is treated as cooperative cancel.
                self.cancel_search_id
                    .store(self.search_id, Ordering::Release);
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        let _ = (
            progress,
            self.lane_id,
            self.base_revision,
            self.position_hash,
        );
    }
}
