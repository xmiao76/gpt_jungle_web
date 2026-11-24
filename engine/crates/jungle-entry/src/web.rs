use crate::*;
use jungle_search::{SearchControl, SearchLimits};
use wasm_bindgen::prelude::*;

fn error(message: impl ToString) -> JsValue {
    JsValue::from_str(&message.to_string())
}
fn state(json: &str) -> Result<Snapshot, JsValue> {
    serde_json::from_str(json).map_err(error)
}
fn encode(value: impl serde::Serialize) -> Result<String, JsValue> {
    serde_json::to_string(&value).map_err(error)
}
#[wasm_bindgen]
pub fn opening(red: bool) -> Result<String, JsValue> {
    encode(Snapshot::opening(if red {
        jungle_core::Side::Red
    } else {
        jungle_core::Side::Blue
    }))
}
#[wasm_bindgen]
pub fn legal_moves(json: &str) -> Result<String, JsValue> {
    encode(state(json)?.legal().map_err(error)?)
}
#[wasm_bindgen]
pub fn apply_move(json: &str, src: u8, dst: u8) -> Result<String, JsValue> {
    encode(state(json)?.apply(Ply { src, dst }).map_err(error)?)
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace=performance,js_name=now)]
    fn now() -> f64;
}
struct Clock {
    progress: Option<js_sys::Function>,
    legal: Vec<Ply>,
}
impl SearchControl for Clock {
    fn now_ms(&self) -> f64 {
        now()
    }
    fn on_iteration(&self, p: &jungle_search::SearchProgress) {
        if let Some(callback) = &self.progress
            && let Some(m) = p.best_move
            && let Some(index) = self.legal.iter().position(|&p| p == Ply::from(m))
        {
            let latest = Choice {
                index,
                nodes: p.nodes,
                depth: p.depth,
                score: p.score,
                proven: false,
            };
            let _ = callback.call1(
                &JsValue::NULL,
                &JsValue::from_str(&serde_json::to_string(&latest).unwrap()),
            );
        }
    }
}
#[wasm_bindgen]
pub fn search(
    json: &str,
    level: &str,
    seed: u32,
    budget: u32,
    deadline: f64,
    progress: Option<js_sys::Function>,
) -> Result<String, JsValue> {
    let state = state(json)?;
    let legal = state.legal().map_err(error)?;
    let result = if deadline <= 0.0 {
        fixed(&state, &legal, level, seed, budget)
    } else {
        let remaining = (deadline - now()).max(1.0) as u32;
        let limits = SearchLimits {
            soft_time_limit_ms: remaining,
            time_limit_ms: remaining,
            ..SearchLimits::fixed_nodes(u64::MAX)
        };
        choose(
            &state,
            &legal,
            level,
            seed,
            limits,
            &Clock {
                progress,
                legal: legal.clone(),
            },
        )
    };
    encode(result.map_err(error)?)
}
