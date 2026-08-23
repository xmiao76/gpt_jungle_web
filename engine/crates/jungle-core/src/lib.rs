//! Canonical, allocation-free Jungle / Dou Shou Qi rules and session state.
//!
//! The rules in this crate intentionally preserve the GPT Jungle variant. In
//! particular Blue moves first, Dog is rank 3, Wolf is rank 4, Lion crosses
//! either river span, Tiger crosses only the two-water-square span, and there
//! is no gameplay repetition or no-capture draw.

mod board;
mod dto;
mod position;
mod session;
mod types;

pub use board::*;
pub use dto::*;
pub use position::*;
pub use session::*;
pub use types::*;

pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const RULESET_ID: &str = "gpt-jungle-custom-v1";
pub const RULES_SEMANTIC_VERSION: u32 = 2;

/// Canonical semantic-v2 manifest used by the legacy two-piece tablebase.
/// Keep byte-for-byte stable unless the gameplay rules change.
pub const RULES_MANIFEST_JSON: &str = concat!(
    "{\"blue_den\":59,\"blue_traps\":[52,58,60],\"board\":[9,7],",
    "\"capture\":{\"elephant_cannot_capture_untrapped_rat\":true,",
    "\"elephant_captures_trapped_rat\":true,\"enemy_trap_rank\":0,",
    "\"equal_or_lower_effective_rank\":true,\"land_rat_captures_elephant\":true,",
    "\"shore_capture_forbidden\":true,\"water_capture\":\"rat-versus-rat-only\"},",
    "\"jumpers\":[6,7],\"movement\":{\"only_rat_enters_water\":true,",
    "\"orthogonal_steps\":true,\"own_den_forbidden\":true,",
    "\"rat_blocks_river_jump\":true},\"red_den\":3,",
    "\"red_traps\":[2,4,10],\"semantic_rules_version\":2,",
    "\"terminal\":[\"opponent-den\",\"capture-all\",\"no-legal-moves\"],",
    "\"tiger_jump_water_span\":2,\"trap_owners\":[[2,\"red\"],[4,\"red\"],",
    "[10,\"red\"],[52,\"blue\"],[58,\"blue\"],[60,\"blue\"]],",
    "\"water\":[22,23,25,26,29,30,32,33,36,37,39,40]}"
);

/// SHA-256 of [`RULES_MANIFEST_JSON`], matching `two_piece_v1.jgtb`.
pub const RULES_HASH: [u8; 32] = [
    0xbd, 0x5e, 0x29, 0xe5, 0xba, 0xf5, 0x01, 0x14, 0xaa, 0x5c, 0xe6, 0x2f, 0x7b, 0xfa, 0xee, 0x32,
    0x5a, 0x4a, 0xab, 0xc4, 0xba, 0x69, 0xe7, 0xb9, 0x83, 0xf3, 0xba, 0x79, 0xc0, 0x0a, 0x0c, 0x3a,
];
pub const RULES_HASH_HEX: &str = "bd5e29e5baf50114aa5ce62f7bfaee325a4aabc4ba69e7b983f3ba79c00a0c3a";
