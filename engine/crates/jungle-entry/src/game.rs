use jungle_core::{BOARD_SIZE, Move, Position, Side, WinReason};
use jungle_search::{
    NoopControl, SearchConfig, SearchControl, SearchEngine, SearchLimits, SearchTermination,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ply {
    pub src: u8,
    pub dst: u8,
}
impl From<Move> for Ply {
    fn from(m: Move) -> Self {
        Self {
            src: m.origin(),
            dst: m.destination(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub rev: u32,
    pub board: Vec<u8>,
    pub first_side: Side,
    pub to_play: Side,
    pub history: Vec<Ply>,
    pub no_capture_plies: u32,
    pub status: String,
    pub winner: Option<Side>,
    pub reason: String,
}
impl Snapshot {
    pub fn opening(first: Side) -> Self {
        Self {
            rev: 0,
            board: Position::initial().board_codes().to_vec(),
            first_side: first,
            to_play: first,
            history: Vec::new(),
            no_capture_plies: 0,
            status: "playing".into(),
            winner: None,
            reason: String::new(),
        }
    }
    // Only the reversible tail can contain an earlier occurrence of today's board.
    // Undo those quiet moves on the supplied board, then replay them to seed search.
    pub fn position(&self) -> Result<Position, String> {
        let mut board: [u8; BOARD_SIZE] = self
            .board
            .as_slice()
            .try_into()
            .map_err(|_| "Expected 63 squares")?;
        let n = self.history.len().min(self.no_capture_plies as usize);
        let tail = &self.history[self.history.len() - n..];
        let mut side = self.to_play;
        for p in tail.iter().rev() {
            if p.src >= 63
                || p.dst >= 63
                || board[p.src as usize] != 0
                || board[p.dst as usize] == 0
            {
                return Err("Invalid quiet history".into());
            }
            board.swap(p.src as usize, p.dst as usize);
            side = side.opponent();
        }
        let mut pos = Position::from_board_codes_checked(board, side).map_err(|e| e.to_string())?;
        pos.set_initial_quiet(self.no_capture_plies - n as u32);
        for p in tail {
            let m = pos
                .find_legal_move(p.src, p.dst)
                .ok_or("Illegal quiet history")?;
            pos.make_move_unchecked(m);
        }
        Ok(pos)
    }
    pub fn legal(&self) -> Result<Vec<Ply>, String> {
        if self.status != "playing" {
            return Ok(vec![]);
        }
        let mut list: Vec<Ply> = self
            .position()?
            .legal_moves()
            .iter()
            .copied()
            .map(Ply::from)
            .collect();
        list.sort_by_key(|p| (p.src, p.dst));
        Ok(list)
    }
    pub fn apply(&self, ply: Ply) -> Result<Self, String> {
        if self.status != "playing" {
            return Err("Game has finished".into());
        }
        let mut pos = self.position()?;
        let m = pos
            .find_legal_move(ply.src, ply.dst)
            .ok_or("Illegal move")?;
        pos.make_move_unchecked(m);
        let mut next = self.clone();
        next.board = pos.board_codes().to_vec();
        next.rev += 1;
        next.history.push(ply);
        next.to_play = pos.side_to_move();
        next.no_capture_plies = pos.quiet_plies();
        let outcome = pos.outcome();
        if let Some(reason) = outcome.win_reason() {
            next.status = "won".into();
            next.winner = outcome.winner();
            next.reason = match reason {
                WinReason::DenEntry => "den",
                WinReason::CaptureAll => "elimination",
                WinReason::NoLegalMoves => "immobilization",
            }
            .into();
        } else if let jungle_core::Outcome::Draw { reason } = outcome {
            next.status = "drawn".into();
            next.reason = reason.as_str().into();
        }
        Ok(next)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    pub index: usize,
    pub nodes: u64,
    pub depth: u8,
    pub proven: bool,
    pub score: i32,
}
pub fn choose(
    state: &Snapshot,
    legal: &[Ply],
    level: &str,
    seed: u32,
    limits: SearchLimits,
    control: &dyn SearchControl,
) -> Result<Choice, String> {
    let mut choice = Choice {
        index: 0,
        nodes: 0,
        depth: 0,
        proven: legal.len() == 1,
        score: 0,
    };
    if legal.len() <= 1 || limits.node_limit == Some(0) {
        return Ok(choice);
    }
    let pos = state.position()?;
    let config = match level {
        "easy" => SearchConfig::easy(),
        "medium" => SearchConfig::medium(),
        _ => SearchConfig::hard(),
    };
    let mut engine = SearchEngine::new(config);
    engine.configure_lane(seed % 4);
    let result = engine
        .choose_move(&pos, limits, control)
        .map_err(|e| e.to_string())?;
    if let Some(m) = result.best_move {
        choice.index = legal.iter().position(|p| *p == Ply::from(m)).unwrap_or(0);
    }
    choice.depth = result.depth;
    choice.nodes = result.nodes;
    choice.score = result.score;
    choice.proven = result.termination == SearchTermination::Proven;
    Ok(choice)
}
pub fn fixed(
    state: &Snapshot,
    legal: &[Ply],
    level: &str,
    seed: u32,
    budget: u32,
) -> Result<Choice, String> {
    let cap = match level {
        "easy" => budget.min(1000),
        "medium" => budget.min(20000),
        _ => budget,
    };
    choose(
        state,
        legal,
        level,
        seed,
        SearchLimits::fixed_nodes(u64::from(cap)),
        &NoopControl,
    )
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub model: String,
    pub code_agent: String,
    pub model_platform: String,
    pub github_url: String,
    pub game_version: String,
    pub runtime: String,
}
impl Identity {
    pub fn load() -> Self {
        serde_json::from_str(include_str!("../../../../identity.json")).expect("identity constants")
    }
    pub fn title(&self) -> String {
        let mut parts = vec![self.model.trim(), self.code_agent.trim()];
        let platform = self.model_platform.trim();
        if !platform.is_empty() && platform != "official" {
            parts.push(platform)
        }
        parts.join(" · ")
    }
    pub fn filename(&self) -> String {
        let mut slug = String::new();
        for ch in self.title().to_ascii_lowercase().chars() {
            if ch.is_ascii_alphanumeric() {
                slug.push(ch)
            } else if !slug.is_empty() && !slug.ends_with('-') {
                slug.push('-')
            }
        }
        format!("{}-{}.wasm", slug.trim_end_matches('-'), self.game_version)
    }
    pub fn details(&self) -> serde_json::Value {
        let mut v = serde_json::to_value(self).unwrap();
        v["title"] = self.title().into();
        v["filename"] = self.filename().into();
        v["rulesId"] = jungle_core::RULESET_ID.into();
        v["rulesDigest"] = jungle_core::RULES_HASH_HEX.into();
        v["engineVersion"] = jungle_core::ENGINE_VERSION.into();
        v
    }
}
