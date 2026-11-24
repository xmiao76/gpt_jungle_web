//! Shared Jungle positions, moves, and search state.
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
pub const RULESET_ID: &str = "desktop-jungle-v2";
pub const RULES_SEMANTIC_VERSION: u32 = 2;
pub const RULES_MANIFEST_JSON: &str = include_str!("../../../../rules/desktop-jungle-v2.json");
pub const RULES_HASH_HEX: &str = "a457bbc009e975a9eafe5ff1f8fa40ca2548f2a689b121c8c4ed99dd0f3fdc57";
pub const RULES_HASH: [u8; 32] = [
    164, 87, 187, 192, 9, 233, 117, 169, 234, 254, 95, 241, 248, 250, 64, 202, 37, 72, 242, 166,
    137, 177, 33, 200, 196, 237, 153, 221, 15, 63, 220, 87,
];
