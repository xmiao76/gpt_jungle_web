use serde::{Deserialize, Serialize};

pub(crate) const ENGINE_NAME: &str = "Jungle Rust Engine";
pub(crate) const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
pub(crate) const PROTOCOL_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct RuntimeFeatures {
    pub simd: bool,
    pub shared_memory: bool,
    pub atomics: bool,
    pub threads: bool,
    pub worker_pool_size: u16,
    pub lazy_smp: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EngineMetadata {
    pub engine_name: &'static str,
    pub engine_version: &'static str,
    pub rules_version: &'static str,
    pub rules_fingerprint: &'static str,
    pub search_version: &'static str,
    pub protocol_version: u32,
    pub save_schema: &'static str,
    pub save_version: u32,
    pub target: &'static str,
}

impl Default for EngineMetadata {
    fn default() -> Self {
        Self {
            engine_name: ENGINE_NAME,
            engine_version: ENGINE_VERSION,
            rules_version: jungle_core::RULESET_ID,
            rules_fingerprint: jungle_core::RULES_HASH_HEX,
            search_version: jungle_search::SEARCH_ENGINE_VERSION,
            protocol_version: PROTOCOL_VERSION,
            save_schema: "gpt-jungle-save",
            save_version: 1,
            target: if cfg!(target_arch = "wasm32") {
                "wasm32-unknown-unknown"
            } else {
                "native"
            },
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CapabilityMetadata {
    pub scalar: bool,
    pub simd_compiled: bool,
    pub simd_available: bool,
    pub atomics_compiled: bool,
    pub atomics_available: bool,
    pub shared_memory_available: bool,
    pub threads_compiled: bool,
    pub threads_available: bool,
    pub lazy_smp_available: bool,
    pub worker_pool_size: u16,
    pub cooperative_cancel: bool,
    pub hard_cancel: &'static str,
    pub progress_callback: bool,
    pub persistent_transposition_table: bool,
    pub exact_tablebase_pieces: u8,
    pub optional_tablebase_resident_cap_bytes: usize,
}

impl CapabilityMetadata {
    pub(crate) fn detect(runtime: RuntimeFeatures) -> Self {
        let simd_compiled = cfg!(target_feature = "simd128");
        let atomics_compiled = cfg!(target_feature = "atomics");
        let threads_compiled = atomics_compiled && cfg!(feature = "threads");
        let atomics_available = atomics_compiled && runtime.atomics;
        let shared_memory_available = atomics_available && runtime.shared_memory;
        // One Wasm session owns one scalar lane. The browser coordinator may
        // truthfully advertise independent Lazy-SMP only after multiple lanes
        // are ready; the shared-memory flag remains specific to the threaded
        // artifact rather than being inferred from lane count alone.
        let worker_pool_size = runtime.worker_pool_size.max(1);
        let threads_available =
            threads_compiled && shared_memory_available && runtime.threads && worker_pool_size > 1;
        let lazy_smp_available = runtime.lazy_smp && worker_pool_size > 1;
        Self {
            scalar: true,
            simd_compiled,
            simd_available: simd_compiled && runtime.simd,
            atomics_compiled,
            atomics_available,
            shared_memory_available,
            threads_compiled,
            threads_available,
            lazy_smp_available,
            worker_pool_size,
            cooperative_cancel: true,
            hard_cancel: "search-worker-termination",
            progress_callback: true,
            persistent_transposition_table: true,
            exact_tablebase_pieces: 2,
            optional_tablebase_resident_cap_bytes: crate::tablebase::OPTIONAL_RESIDENT_CAP_BYTES,
        }
    }
}

pub(crate) fn metadata_json() -> String {
    serde_json::to_string(&EngineMetadata::default()).expect("metadata is serializable")
}

pub(crate) fn capabilities_json(runtime: RuntimeFeatures) -> String {
    serde_json::to_string(&CapabilityMetadata::detect(runtime))
        .expect("capability metadata is serializable")
}
