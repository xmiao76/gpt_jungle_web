#![forbid(unsafe_code)]

mod control;
mod evaluator;
mod search;
pub mod transposition;
mod types;

pub use control::{NoopControl, SearchControl, SystemControl};
pub use evaluator::{ClassicalEvaluator, Evaluator, QuantizedEvaluator, QuantizedModelError};
pub use search::{
    EndgameOracle, OracleHit, OracleOutcome, SEARCH_ENGINE_VERSION, SearchConfig, SearchEngine,
    SearchError,
};
pub use types::{
    DEFAULT_HARD_TIME_MS, DEFAULT_SOFT_TIME_MS, MAX_SEARCH_DEPTH, SearchDiagnostics, SearchLimits,
    SearchProgress, SearchResult, SearchTermination,
};
