#![forbid(unsafe_code)]

mod bridge;
mod control;
mod error;
mod metadata;
mod protocol;
mod tablebase;

pub use bridge::WasmGameSession;
use wasm_bindgen::prelude::wasm_bindgen;

/// Static engine metadata for bootstrapping before a session is constructed.
#[wasm_bindgen]
pub fn engine_metadata_json() -> String {
    metadata::metadata_json()
}

/// Build-time capabilities. A session's `capabilities_json` additionally
/// incorporates browser feature detection supplied by the Worker.
#[wasm_bindgen]
pub fn build_capabilities_json() -> String {
    metadata::capabilities_json(metadata::RuntimeFeatures::default())
}
