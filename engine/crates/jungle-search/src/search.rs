//! Strength-oriented iterative-deepening search shared by native and Wasm builds.

use std::sync::Arc;

use jungle_core::{Move, Outcome, Position, Side, WinReason, opponent_den, own_den, trap_owner};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::transposition::{Bound, TranspositionTable, score_from_table, score_to_table};
use crate::{
    ClassicalEvaluator, Evaluator, MAX_SEARCH_DEPTH, SearchControl, SearchDiagnostics,
    SearchLimits, SearchProgress, SearchResult, SearchTermination,
};

pub const SEARCH_ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

const INFINITY: i32 = 120_000;
const MATE_SCORE: i32 = 100_000;
const MATE_THRESHOLD: i32 = MATE_SCORE - 1_000;
const TABLEBASE_SCORE: i32 = 98_000;
const MAX_PLY: usize = MAX_SEARCH_DEPTH as usize + 24;
const COUNTERMOVE_SLOTS: usize = 4_096;
const HISTORY_MAX: i32 = 32_000;
const PIECE_VALUES: [i32; 9] = [0, 165, 250, 335, 420, 535, 790, 860, 930];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchConfig {
    pub transposition_bytes: usize,
    pub aspiration_window: i32,
    pub quiescence_depth: u8,
    pub time_check_interval: u32,
}

impl SearchConfig {
    #[must_use]
    pub const fn easy() -> Self {
        Self {
            transposition_bytes: 1 << 20,
            aspiration_window: 192,
            quiescence_depth: 1,
            time_check_interval: 64,
        }
    }

    #[must_use]
    pub const fn medium() -> Self {
        Self {
            transposition_bytes: 4 << 20,
            aspiration_window: 176,
            quiescence_depth: 2,
            time_check_interval: 48,
        }
    }

    #[must_use]
    pub const fn hard() -> Self {
        Self {
            transposition_bytes: 16 << 20,
            aspiration_window: 160,
            quiescence_depth: 2,
            time_check_interval: 32,
        }
    }
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self::hard()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OracleOutcome {
    Win,
    Draw,
    Loss,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OracleHit {
    pub outcome: OracleOutcome,
    pub distance: Option<u16>,
}

pub trait EndgameOracle: Send + Sync {
    /// Exact result from the current side-to-move's perspective.
    fn probe(&self, position: &Position) -> Option<OracleHit>;
}

#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum SearchError {
    #[error("position failed its consistency check")]
    InvalidPosition,
}

/// Persistent owner for reusable ordering and transposition state.
pub struct SearchEngine {
    config: SearchConfig,
    evaluator: Arc<dyn Evaluator>,
    oracle: Option<Arc<dyn EndgameOracle>>,
    table: TranspositionTable,
    history: Box<[[[i32; 63]; 63]; 2]>,
    countermoves: Box<[u32; COUNTERMOVE_SLOTS]>,
    lane_id: u32,
}

impl Default for SearchEngine {
    fn default() -> Self {
        Self::new(SearchConfig::default())
    }
}

impl SearchEngine {
    #[must_use]
    pub fn new(config: SearchConfig) -> Self {
        Self::with_evaluator(config, Arc::new(ClassicalEvaluator))
    }

    #[must_use]
    pub fn with_evaluator(config: SearchConfig, evaluator: Arc<dyn Evaluator>) -> Self {
        Self {
            table: TranspositionTable::with_memory_budget(config.transposition_bytes),
            config,
            evaluator,
            oracle: None,
            history: Box::new([[[0; 63]; 63]; 2]),
            countermoves: Box::new([0; COUNTERMOVE_SLOTS]),
            lane_id: 0,
        }
    }

    #[must_use]
    pub fn with_oracle(config: SearchConfig, oracle: Arc<dyn EndgameOracle>) -> Self {
        let mut engine = Self::new(config);
        engine.oracle = Some(oracle);
        engine
    }

    #[must_use]
    pub const fn config(&self) -> SearchConfig {
        self.config
    }

    #[must_use]
    pub const fn lane_id(&self) -> u32 {
        self.lane_id
    }

    /// Diversifies only equal-priority ordering decisions. Search values and
    /// rules remain deterministic for a fixed lane.
    pub fn configure_lane(&mut self, lane_id: u32) {
        self.lane_id = lane_id;
    }

    #[must_use]
    pub fn evaluator_id(&self) -> &'static str {
        self.evaluator.id()
    }

    #[must_use]
    pub fn has_endgame_oracle(&self) -> bool {
        self.oracle.is_some()
    }

    #[must_use]
    pub fn tablebase_loaded(&self) -> bool {
        self.has_endgame_oracle()
    }

    #[must_use]
    pub fn tt_memory_bytes(&self) -> usize {
        self.table.memory_bytes()
    }

    #[must_use]
    pub fn tt_capacity(&self) -> usize {
        self.table.entry_capacity()
    }

    #[must_use]
    pub const fn lazy_smp_supported(&self) -> bool {
        true
    }

    #[must_use]
    pub const fn lazy_smp_active(&self) -> bool {
        self.lane_id != 0
    }

    pub fn set_endgame_oracle(&mut self, oracle: Option<Arc<dyn EndgameOracle>>) {
        self.oracle = oracle;
        self.table.clear();
    }

    pub fn set_evaluator(&mut self, evaluator: Arc<dyn Evaluator>) {
        self.evaluator = evaluator;
        self.table.clear();
        self.history.fill([[0; 63]; 63]);
        self.countermoves.fill(0);
    }

    pub fn reconfigure(&mut self, config: SearchConfig) {
        if config.transposition_bytes != self.config.transposition_bytes {
            self.table = TranspositionTable::with_memory_budget(config.transposition_bytes);
        }
        self.config = config;
    }

    /// Clears reusable search state while preserving the evaluator, oracle,
    /// configuration, and assigned Lazy-SMP lane.
    pub fn reset(&mut self) {
        self.table.clear();
        self.history.fill([[0; 63]; 63]);
        self.countermoves.fill(0);
    }

    pub fn choose_move(
        &mut self,
        position: &Position,
        limits: SearchLimits,
        control: &dyn SearchControl,
    ) -> Result<SearchResult, SearchError> {
        if !position.is_consistent() {
            return Err(SearchError::InvalidPosition);
        }

        self.table.new_search();
        age_history(&mut self.history);
        let limits = limits.normalized();
        let started_ms = control.now_ms();
        let root_oracle_hit = self
            .oracle
            .as_deref()
            .and_then(|oracle| oracle.probe(position));

        let evaluator = Arc::clone(&self.evaluator);
        let oracle = self.oracle.clone();
        let mut session = SearchSession::new(
            evaluator.as_ref(),
            oracle.as_deref(),
            &mut self.table,
            &mut self.history,
            &mut self.countermoves,
            control,
            self.config,
            limits,
            started_ms,
            self.lane_id,
        );
        let mut board = position.clone();
        let legal = board.legal_moves();
        if legal.is_empty() {
            let outcome = board.outcome_from_legal_moves(&legal);
            let termination = match outcome {
                Outcome::Win {
                    reason: WinReason::NoLegalMoves,
                    ..
                } => SearchTermination::NoLegalMove,
                _ => SearchTermination::Terminal,
            };
            return Ok(session.finish(
                None,
                terminal_score(outcome, board.side_to_move(), 0),
                0,
                Vec::new(),
                Vec::new(),
                termination,
            ));
        }

        let fallback = canonical_first(legal.as_slice());
        let mut completed_move = fallback;
        let mut completed_score = 0;
        let mut completed_depth = 0;
        let mut completed_pv = vec![fallback];
        let mut iterations = Vec::new();
        let mut termination = SearchTermination::MaxDepth;

        for depth in 1..=limits.max_depth.min(MAX_SEARCH_DEPTH) {
            if session.soft_deadline_reached() && completed_depth > 0 {
                termination = SearchTermination::SoftDeadline;
                break;
            }

            let root_result = session.iteration_with_aspiration(
                &mut board,
                legal.as_slice(),
                depth,
                completed_score,
            );
            match root_result {
                Ok(result) => {
                    completed_depth = depth;
                    completed_score = result.score;
                    completed_move = result.best_move;
                    completed_pv = result.pv;
                    let progress = SearchProgress {
                        depth,
                        selective_depth: session.selective_depth,
                        nodes: session.nodes,
                        elapsed_ms: session.elapsed_ms(),
                        score: completed_score,
                        best_move: Some(completed_move),
                        principal_variation: completed_pv.clone(),
                    };
                    control.on_iteration(&progress);
                    iterations.push(progress);

                    if completed_score.abs() >= MATE_THRESHOLD || root_oracle_hit.is_some() {
                        termination = SearchTermination::Proven;
                        break;
                    }
                    if session.soft_deadline_reached() {
                        termination = SearchTermination::SoftDeadline;
                        break;
                    }
                }
                Err(stop) => {
                    termination = stop.termination();
                    break;
                }
            }
        }

        Ok(session.finish(
            Some(completed_move),
            completed_score,
            completed_depth,
            completed_pv,
            iterations,
            termination,
        ))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Stop {
    Nodes,
    HardDeadline,
    Cancelled,
}

impl Stop {
    const fn termination(self) -> SearchTermination {
        match self {
            Self::Nodes => SearchTermination::NodeLimit,
            Self::HardDeadline => SearchTermination::HardDeadline,
            Self::Cancelled => SearchTermination::Cancelled,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct NodeValue {
    score: i32,
    history_dependent: bool,
}

impl NodeValue {
    const fn independent(score: i32) -> Self {
        Self {
            score,
            history_dependent: false,
        }
    }

    const fn cycle() -> Self {
        Self {
            score: 0,
            history_dependent: true,
        }
    }
}

#[derive(Debug)]
struct RootResult {
    score: i32,
    best_move: Move,
    pv: Vec<Move>,
}

struct SearchSession<'a> {
    evaluator: &'a dyn Evaluator,
    oracle: Option<&'a dyn EndgameOracle>,
    table: &'a mut TranspositionTable,
    history: &'a mut [[[i32; 63]; 63]; 2],
    countermoves: &'a mut [u32; COUNTERMOVE_SLOTS],
    control: &'a dyn SearchControl,
    config: SearchConfig,
    limits: SearchLimits,
    started_ms: f64,
    nodes: u64,
    selective_depth: u8,
    diagnostics: SearchDiagnostics,
    killers: [[u32; 2]; MAX_PLY],
    pv: Vec<Vec<Option<Move>>>,
    pv_lengths: Vec<usize>,
    lane_id: u32,
}

impl<'a> SearchSession<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        evaluator: &'a dyn Evaluator,
        oracle: Option<&'a dyn EndgameOracle>,
        table: &'a mut TranspositionTable,
        history: &'a mut [[[i32; 63]; 63]; 2],
        countermoves: &'a mut [u32; COUNTERMOVE_SLOTS],
        control: &'a dyn SearchControl,
        config: SearchConfig,
        limits: SearchLimits,
        started_ms: f64,
        lane_id: u32,
    ) -> Self {
        Self {
            evaluator,
            oracle,
            table,
            history,
            countermoves,
            control,
            config,
            limits,
            started_ms,
            nodes: 0,
            selective_depth: 0,
            diagnostics: SearchDiagnostics::default(),
            killers: [[0; 2]; MAX_PLY],
            pv: vec![vec![None; MAX_PLY]; MAX_PLY],
            pv_lengths: vec![0; MAX_PLY],
            lane_id,
        }
    }

    fn elapsed_ms(&self) -> f64 {
        (self.control.now_ms() - self.started_ms).max(0.0)
    }

    fn soft_deadline_reached(&self) -> bool {
        self.elapsed_ms() >= f64::from(self.limits.soft_time_limit_ms)
    }

    fn enter_node(&mut self, ply: usize, qnode: bool) -> Result<(), Stop> {
        if self.control.should_stop() {
            return Err(Stop::Cancelled);
        }
        if self
            .limits
            .node_limit
            .is_some_and(|limit| self.nodes >= limit)
        {
            return Err(Stop::Nodes);
        }
        let interval = u64::from(self.config.time_check_interval.max(1));
        if self.nodes.is_multiple_of(interval)
            && self.elapsed_ms() >= f64::from(self.limits.time_limit_ms)
        {
            return Err(Stop::HardDeadline);
        }
        self.nodes += 1;
        if qnode {
            self.diagnostics.qnodes += 1;
        }
        self.selective_depth = self
            .selective_depth
            .max(u8::try_from(ply).unwrap_or(u8::MAX));
        Ok(())
    }

    fn iteration_with_aspiration(
        &mut self,
        board: &mut Position,
        moves: &[Move],
        depth: u8,
        previous_score: i32,
    ) -> Result<RootResult, Stop> {
        if depth == 1 || previous_score.abs() >= MATE_THRESHOLD {
            return self.search_root(board, moves, depth, -INFINITY, INFINITY);
        }

        let mut window = self.config.aspiration_window.max(16);
        let mut alpha = (previous_score - window).max(-INFINITY);
        let mut beta = (previous_score + window).min(INFINITY);
        loop {
            let result = self.search_root(board, moves, depth, alpha, beta)?;
            if result.score <= alpha {
                self.diagnostics.aspiration_researches += 1;
                window = window.saturating_mul(2).min(INFINITY);
                alpha = (result.score - window).max(-INFINITY);
                beta = (previous_score + window).min(INFINITY);
            } else if result.score >= beta {
                self.diagnostics.aspiration_researches += 1;
                window = window.saturating_mul(2).min(INFINITY);
                alpha = (previous_score - window).max(-INFINITY);
                beta = (result.score + window).min(INFINITY);
            } else {
                return Ok(result);
            }
            if alpha == -INFINITY && beta == INFINITY {
                return self.search_root(board, moves, depth, alpha, beta);
            }
        }
    }

    fn search_root(
        &mut self,
        board: &mut Position,
        moves: &[Move],
        depth: u8,
        mut alpha: i32,
        beta: i32,
    ) -> Result<RootResult, Stop> {
        self.enter_node(0, false)?;
        self.clear_pv(0);
        let original_alpha = alpha;
        let root_key = board.search_key();
        let table_move = self
            .table
            .probe(root_key)
            .and_then(|entry| Move::from_packed(entry.best_move));
        let mut ordered = moves.to_vec();
        self.order_moves(&mut ordered, table_move, 0, None);
        let mut best_move = ordered[0];
        let mut best_score = -INFINITY;
        let mut history_dependent = false;

        for (index, movement) in ordered.into_iter().enumerate() {
            let extension = self.extension_for(movement, 0);
            let child_depth = i16::from(depth).saturating_sub(1) + i16::from(extension > 0);
            let undo = board.make_move_unchecked(movement);
            let child_result = (|| -> Result<NodeValue, Stop> {
                if index == 0 {
                    self.negamax(
                        board,
                        child_depth,
                        -beta,
                        -alpha,
                        1,
                        true,
                        extension,
                        Some(movement),
                    )
                } else {
                    let mut value = self.negamax(
                        board,
                        child_depth,
                        -alpha - 1,
                        -alpha,
                        1,
                        false,
                        extension,
                        Some(movement),
                    )?;
                    value.score = -value.score;
                    if value.score > alpha && value.score < beta {
                        self.diagnostics.pvs_researches += 1;
                        let mut researched = self.negamax(
                            board,
                            child_depth,
                            -beta,
                            -alpha,
                            1,
                            true,
                            extension,
                            Some(movement),
                        )?;
                        researched.score = -researched.score;
                        value = researched;
                    }
                    Ok(value)
                }
            })();
            board.unmake_move(movement, undo);
            let mut child = child_result?;
            if index == 0 {
                child.score = -child.score;
            }
            let score = child.score;
            history_dependent |= child.history_dependent;

            if score > best_score {
                best_score = score;
                best_move = movement;
                self.update_pv(0, movement);
            }
            alpha = alpha.max(score);
            if alpha >= beta {
                self.diagnostics.beta_cutoffs += 1;
                break;
            }
        }

        let bound = if best_score <= original_alpha {
            Bound::Upper
        } else if best_score >= beta {
            Bound::Lower
        } else {
            Bound::Exact
        };
        self.table.store(
            root_key,
            i16::from(depth),
            score_to_table(best_score, 0, MATE_THRESHOLD),
            bound,
            best_move.packed(),
            !history_dependent,
        );
        let pv = self.pv_line(0);
        Ok(RootResult {
            score: best_score,
            best_move,
            pv: if pv.is_empty() { vec![best_move] } else { pv },
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn negamax(
        &mut self,
        board: &mut Position,
        depth: i16,
        alpha: i32,
        beta: i32,
        ply: usize,
        pv_node: bool,
        extensions_used: u8,
        previous_move: Option<Move>,
    ) -> Result<NodeValue, Stop> {
        self.enter_node(ply, false)?;
        self.clear_pv(ply);
        if ply + 1 >= MAX_PLY {
            self.diagnostics.evaluations += 1;
            return Ok(NodeValue::independent(self.evaluator.evaluate(board)));
        }

        let key = board.search_key();
        if let Some(hit) = self.oracle.and_then(|oracle| oracle.probe(board)) {
            self.diagnostics.tablebase_hits += 1;
            return Ok(NodeValue::independent(oracle_score(hit, ply)));
        }
        if board.draw_reason().is_some() && matches!(board.outcome(), Outcome::Draw { .. }) {
            self.diagnostics.cycle_hits += 1;
            return Ok(NodeValue::cycle());
        }

        self.negamax_body(
            board,
            depth,
            alpha,
            beta,
            ply,
            pv_node,
            extensions_used,
            previous_move,
            key,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn negamax_body(
        &mut self,
        board: &mut Position,
        depth: i16,
        mut alpha: i32,
        beta: i32,
        ply: usize,
        pv_node: bool,
        extensions_used: u8,
        previous_move: Option<Move>,
        key: u64,
    ) -> Result<NodeValue, Stop> {
        let moves = board.legal_moves();
        if moves.is_empty() {
            return Ok(NodeValue::independent(terminal_score(
                board.outcome_from_legal_moves(&moves),
                board.side_to_move(),
                ply,
            )));
        }
        if depth <= 0 {
            self.diagnostics.qnodes += 1;
            return self.quiescence_body(
                board,
                alpha,
                beta,
                ply,
                self.config.quiescence_depth,
                previous_move,
                &moves,
            );
        }

        let original_alpha = alpha;
        let generation = self.table.generation();
        let table_entry = self.table.probe(key);
        let table_move = table_entry.and_then(|entry| Move::from_packed(entry.best_move));
        if let Some(entry) = table_entry
            && entry.generation == generation
            && entry.history_safe
            && entry.depth >= depth
        {
            let score = score_from_table(entry.score, ply as u16, MATE_THRESHOLD);
            let cutoff = match entry.bound {
                Bound::Exact => true,
                Bound::Lower => score >= beta,
                Bound::Upper => score <= alpha,
            };
            if cutoff {
                self.diagnostics.tt_cutoffs += 1;
                if let Some(movement) = table_move {
                    self.pv[ply][0] = Some(movement);
                    self.pv_lengths[ply] = 1;
                }
                return Ok(NodeValue::independent(score));
            }
        }

        let mut ordered = moves.as_slice().to_vec();
        self.order_moves(&mut ordered, table_move, ply, previous_move);
        let mut best_score = -INFINITY;
        let mut best_move = ordered[0];
        let mut any_history_dependency = false;

        for (index, movement) in ordered.into_iter().enumerate() {
            let quiet = movement.captured().is_none() && !is_den_pressure(movement);
            let extension = u8::from(extensions_used == 0 && is_den_pressure(movement));
            let full_depth = depth - 1 + i16::from(extension);
            let reduction = lmr_reduction(depth, index, quiet, pv_node, extension != 0);
            let reduced_depth = (full_depth - i16::from(reduction)).max(0);
            let undo = board.make_move_unchecked(movement);

            let child_result = (|| -> Result<NodeValue, Stop> {
                if index == 0 {
                    self.negamax(
                        board,
                        full_depth,
                        -beta,
                        -alpha,
                        ply + 1,
                        pv_node,
                        extensions_used + extension,
                        Some(movement),
                    )
                } else {
                    let mut value = self.negamax(
                        board,
                        reduced_depth,
                        -alpha - 1,
                        -alpha,
                        ply + 1,
                        false,
                        extensions_used + extension,
                        Some(movement),
                    )?;
                    value.score = -value.score;
                    if reduction > 0 && value.score > alpha {
                        self.diagnostics.lmr_researches += 1;
                        let mut researched = self.negamax(
                            board,
                            full_depth,
                            -alpha - 1,
                            -alpha,
                            ply + 1,
                            false,
                            extensions_used + extension,
                            Some(movement),
                        )?;
                        researched.score = -researched.score;
                        value = researched;
                    }
                    if value.score > alpha && value.score < beta {
                        self.diagnostics.pvs_researches += 1;
                        let mut researched = self.negamax(
                            board,
                            full_depth,
                            -beta,
                            -alpha,
                            ply + 1,
                            pv_node,
                            extensions_used + extension,
                            Some(movement),
                        )?;
                        researched.score = -researched.score;
                        value = researched;
                    }
                    Ok(value)
                }
            })();
            board.unmake_move(movement, undo);
            let mut child = child_result?;
            if index == 0 {
                child.score = -child.score;
            }
            any_history_dependency |= child.history_dependent;
            let score = child.score;

            if score > best_score {
                best_score = score;
                best_move = movement;
                self.update_pv(ply, movement);
            }
            alpha = alpha.max(score);
            if alpha >= beta {
                self.diagnostics.beta_cutoffs += 1;
                if quiet {
                    self.record_quiet_cutoff(movement, previous_move, ply, depth);
                }
                break;
            }
        }

        let bound = if best_score <= original_alpha {
            Bound::Upper
        } else if best_score >= beta {
            Bound::Lower
        } else {
            Bound::Exact
        };
        self.table.store(
            key,
            depth,
            score_to_table(best_score, ply as u16, MATE_THRESHOLD),
            bound,
            best_move.packed(),
            !any_history_dependency,
        );
        Ok(NodeValue {
            score: best_score,
            history_dependent: any_history_dependency,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn quiescence(
        &mut self,
        board: &mut Position,
        alpha: i32,
        beta: i32,
        ply: usize,
        remaining: u8,
        previous_move: Option<Move>,
    ) -> Result<NodeValue, Stop> {
        self.enter_node(ply, true)?;
        self.clear_pv(ply);
        if ply + 1 >= MAX_PLY {
            self.diagnostics.evaluations += 1;
            return Ok(NodeValue::independent(self.evaluator.evaluate(board)));
        }
        if let Some(hit) = self.oracle.and_then(|oracle| oracle.probe(board)) {
            self.diagnostics.tablebase_hits += 1;
            return Ok(NodeValue::independent(oracle_score(hit, ply)));
        }
        if board.draw_reason().is_some() && matches!(board.outcome(), Outcome::Draw { .. }) {
            self.diagnostics.cycle_hits += 1;
            return Ok(NodeValue::cycle());
        }

        let moves = board.legal_moves();
        if moves.is_empty() {
            Ok(NodeValue::independent(terminal_score(
                board.outcome_from_legal_moves(&moves),
                board.side_to_move(),
                ply,
            )))
        } else {
            self.quiescence_body(board, alpha, beta, ply, remaining, previous_move, &moves)
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn quiescence_body(
        &mut self,
        board: &mut Position,
        mut alpha: i32,
        beta: i32,
        ply: usize,
        remaining: u8,
        previous_move: Option<Move>,
        legal_moves: &jungle_core::MoveList,
    ) -> Result<NodeValue, Stop> {
        self.diagnostics.evaluations += 1;
        let under_den_threat = has_immediate_den_threat(board);
        let stand_pat = self.evaluator.evaluate(board);
        if !under_den_threat {
            if stand_pat >= beta {
                return Ok(NodeValue::independent(stand_pat));
            }
            alpha = alpha.max(stand_pat);
        }
        if remaining == 0 {
            return Ok(NodeValue::independent(if under_den_threat {
                stand_pat
            } else {
                alpha
            }));
        }

        let mut tactical: Vec<Move> = if under_den_threat {
            let mut defenses = Vec::new();
            for &movement in legal_moves {
                self.enter_node(ply + 1, true)?;
                if defends_den(board, movement) {
                    defenses.push(movement);
                }
            }
            defenses
        } else {
            let target_den = opponent_den(board.side_to_move());
            legal_moves
                .iter()
                .copied()
                .filter(|movement| {
                    movement.captured().is_some() || movement.destination() == target_den
                })
                .collect()
        };
        if under_den_threat && tactical.is_empty() {
            return Ok(NodeValue::independent(
                -MATE_SCORE + i32::try_from(ply).unwrap_or(999).min(999),
            ));
        }
        self.order_moves(&mut tactical, None, ply, previous_move);
        let mut best = alpha;
        let mut history_dependent = false;
        for movement in tactical {
            let undo = board.make_move_unchecked(movement);
            let child =
                self.quiescence(board, -beta, -best, ply + 1, remaining - 1, Some(movement));
            board.unmake_move(movement, undo);
            let child = child?;
            let score = -child.score;
            history_dependent |= child.history_dependent;
            if score > best {
                best = score;
                self.update_pv(ply, movement);
            }
            if best >= beta {
                self.diagnostics.beta_cutoffs += 1;
                break;
            }
        }
        Ok(NodeValue {
            score: best,
            history_dependent,
        })
    }

    fn extension_for(&self, movement: Move, used: u8) -> u8 {
        u8::from(used == 0 && is_den_pressure(movement))
    }

    fn order_moves(
        &self,
        moves: &mut [Move],
        table_move: Option<Move>,
        ply: usize,
        previous_move: Option<Move>,
    ) {
        let counter = previous_move
            .map(countermove_index)
            .map_or(0, |index| self.countermoves[index]);
        moves.sort_unstable_by(|left, right| {
            let left_score = self.move_order_score(*left, table_move, ply, counter);
            let right_score = self.move_order_score(*right, table_move, ply, counter);
            right_score
                .cmp(&left_score)
                .then_with(|| self.tie_key(*left).cmp(&self.tie_key(*right)))
        });
    }

    fn move_order_score(
        &self,
        movement: Move,
        table_move: Option<Move>,
        ply: usize,
        counter: u32,
    ) -> i32 {
        if table_move == Some(movement) {
            return 2_000_000;
        }
        if movement.destination() == opponent_den(movement.piece().side()) {
            return 1_500_000;
        }
        if let Some(victim) = movement.captured() {
            let victim_value = PIECE_VALUES[victim.kind().rank() as usize];
            let attacker_value = PIECE_VALUES[movement.piece().kind().rank() as usize];
            return 200_000 + victim_value * 32 - attacker_value;
        }
        let mut score =
            (18 - i32::from(manhattan(
                movement.destination(),
                opponent_den(movement.piece().side()),
            ))) * 100;
        if trap_owner(movement.destination()) == Some(movement.piece().side().opponent()) {
            score += 18_000;
        }
        if movement.is_jump() {
            score += 2_500;
        }
        if ply < MAX_PLY {
            if self.killers[ply][0] == movement.packed() {
                score += 35_000;
            } else if self.killers[ply][1] == movement.packed() {
                score += 30_000;
            }
        }
        if counter == movement.packed() {
            score += 25_000;
        }
        score
            + self.history[movement.piece().side().index()][movement.origin() as usize]
                [movement.destination() as usize]
    }

    fn record_quiet_cutoff(
        &mut self,
        movement: Move,
        previous_move: Option<Move>,
        ply: usize,
        depth: i16,
    ) {
        if ply < MAX_PLY && self.killers[ply][0] != movement.packed() {
            self.killers[ply][1] = self.killers[ply][0];
            self.killers[ply][0] = movement.packed();
        }
        if let Some(previous) = previous_move {
            self.countermoves[countermove_index(previous)] = movement.packed();
        }
        let entry = &mut self.history[movement.piece().side().index()][movement.origin() as usize]
            [movement.destination() as usize];
        let bonus = (i32::from(depth.max(1)).pow(2) * 32).min(2_048);
        *entry += bonus - (*entry * bonus.abs() / HISTORY_MAX);
        *entry = (*entry).clamp(-HISTORY_MAX, HISTORY_MAX);
    }

    fn tie_key(&self, movement: Move) -> u64 {
        if self.lane_id == 0 {
            u64::from(movement.packed())
        } else {
            mix64(
                u64::from(movement.packed())
                    ^ u64::from(self.lane_id).wrapping_mul(0x9e37_79b9_7f4a_7c15),
            )
        }
    }

    fn clear_pv(&mut self, ply: usize) {
        if ply < MAX_PLY {
            self.pv_lengths[ply] = 0;
        }
    }

    fn update_pv(&mut self, ply: usize, movement: Move) {
        if ply + 1 >= MAX_PLY {
            return;
        }
        self.pv[ply][0] = Some(movement);
        let child_len = self.pv_lengths[ply + 1].min(MAX_PLY - 1);
        for index in 0..child_len {
            self.pv[ply][index + 1] = self.pv[ply + 1][index];
        }
        self.pv_lengths[ply] = child_len + 1;
    }

    fn pv_line(&self, ply: usize) -> Vec<Move> {
        self.pv[ply][..self.pv_lengths[ply]]
            .iter()
            .flatten()
            .copied()
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn finish(
        mut self,
        best_move: Option<Move>,
        score: i32,
        depth: u8,
        principal_variation: Vec<Move>,
        iterations: Vec<SearchProgress>,
        termination: SearchTermination,
    ) -> SearchResult {
        let elapsed_ms = self.elapsed_ms();
        let table_stats = self.table.stats();
        self.diagnostics.tt_probes = table_stats.probes;
        self.diagnostics.tt_hits = table_stats.hits;
        self.diagnostics.tt_stores = table_stats.stores;
        self.diagnostics.tt_collisions = table_stats.collisions;
        self.diagnostics.hashfull_per_mille = self.table.hashfull_per_mille();
        SearchResult {
            best_move,
            score,
            depth,
            selective_depth: self.selective_depth,
            nodes: self.nodes,
            elapsed_ms,
            nodes_per_second: if elapsed_ms > 0.0 {
                (self.nodes as f64 * 1_000.0 / elapsed_ms) as u64
            } else {
                0
            },
            transposition_hits: self.diagnostics.tt_hits,
            tablebase_hits: self.diagnostics.tablebase_hits,
            principal_variation,
            iterations,
            termination,
            diagnostics: self.diagnostics,
        }
    }
}

fn age_history(history: &mut [[[i32; 63]; 63]; 2]) {
    for side in history {
        for origins in side {
            for value in origins {
                *value = *value * 3 / 4;
            }
        }
    }
}

fn canonical_first(moves: &[Move]) -> Move {
    moves
        .iter()
        .copied()
        .min_by_key(|movement| movement.packed())
        .expect("a nonterminal position has a legal move")
}

fn countermove_index(movement: Move) -> usize {
    ((movement.origin() as usize * 63) + movement.destination() as usize) & (COUNTERMOVE_SLOTS - 1)
}

fn lmr_reduction(depth: i16, move_index: usize, quiet: bool, pv_node: bool, extended: bool) -> u8 {
    if depth < 4 || move_index < 5 || !quiet || extended {
        return 0;
    }
    let mut reduction = 1;
    if depth >= 8 && move_index >= 10 {
        reduction += 1;
    }
    if !pv_node && depth >= 12 && move_index >= 16 {
        reduction += 1;
    }
    reduction
}

fn is_den_pressure(movement: Move) -> bool {
    movement.destination() == opponent_den(movement.piece().side())
        || trap_owner(movement.destination()) == Some(movement.piece().side().opponent())
}

fn has_immediate_den_threat(position: &Position) -> bool {
    let den = own_den(position.side_to_move());
    position
        .legal_moves_for(position.side_to_move().opponent())
        .iter()
        .any(|movement| movement.destination() == den)
}

fn defends_den(position: &mut Position, movement: Move) -> bool {
    let mover = position.side_to_move();
    let undo = position.make_move_unchecked(movement);
    let defended = position.outcome().is_over()
        || !position
            .legal_moves()
            .iter()
            .any(|reply| reply.destination() == own_den(mover));
    position.unmake_move(movement, undo);
    defended
}

fn manhattan(first: u8, second: u8) -> u8 {
    let (first_row, first_col) = (first / 7, first % 7);
    let (second_row, second_col) = (second / 7, second % 7);
    first_row.abs_diff(second_row) + first_col.abs_diff(second_col)
}

fn oracle_score(hit: OracleHit, ply: usize) -> i32 {
    let distance = i32::from(hit.distance.unwrap_or(0)) + i32::try_from(ply).unwrap_or(i32::MAX);
    match hit.outcome {
        OracleOutcome::Win => TABLEBASE_SCORE - distance,
        OracleOutcome::Draw => 0,
        OracleOutcome::Loss => -TABLEBASE_SCORE + distance,
    }
}

fn terminal_score(outcome: Outcome, side_to_move: Side, ply: usize) -> i32 {
    let distance = i32::try_from(ply).unwrap_or(i32::MAX).min(999);
    match outcome {
        Outcome::Win { winner, .. } if winner == side_to_move => MATE_SCORE - distance,
        Outcome::Win { .. } => -MATE_SCORE + distance,
        Outcome::Ongoing | Outcome::Draw { .. } => 0,
    }
}

fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NoopControl;
    use jungle_core::{Piece, PieceKind};

    fn naive_negamax(position: &mut Position, depth: u8) -> i32 {
        let moves = position.legal_moves();
        if moves.is_empty() {
            return terminal_score(
                position.outcome_from_legal_moves(&moves),
                position.side_to_move(),
                0,
            );
        }
        if depth == 0 {
            return ClassicalEvaluator.evaluate(position);
        }
        moves
            .iter()
            .copied()
            .map(|movement| {
                let undo = position.make_move_unchecked(movement);
                let score = -naive_negamax(position, depth - 1);
                position.unmake_move(movement, undo);
                score
            })
            .max()
            .unwrap()
    }

    #[test]
    fn fixed_node_search_is_legal_bounded_and_repeatable() {
        let position = Position::initial();
        let mut first = SearchEngine::new(SearchConfig::easy());
        let mut second = SearchEngine::new(SearchConfig::easy());
        let limits = SearchLimits::fixed_nodes(8_000);
        let left = first.choose_move(&position, limits, &NoopControl).unwrap();
        let right = second.choose_move(&position, limits, &NoopControl).unwrap();
        assert!(left.nodes <= 8_000);
        assert_eq!(left.best_move, right.best_move);
        assert_eq!(left.score, right.score);
        assert!(
            position
                .legal_moves()
                .iter()
                .any(|movement| Some(*movement) == left.best_move)
        );
        assert!(!left.iterations.is_empty());
    }

    #[test]
    fn shallow_scores_match_unpruned_negamax() {
        let position = Position::initial();
        for depth in 2..=3 {
            let mut expected_position = position.clone();
            let expected = naive_negamax(&mut expected_position, depth);
            let mut config = SearchConfig::easy();
            config.quiescence_depth = 0;
            let mut limits = SearchLimits::fixed_nodes(1_000_000);
            limits.max_depth = depth;
            let result = SearchEngine::new(config)
                .choose_move(&position, limits, &NoopControl)
                .unwrap();
            assert_eq!(result.depth, depth);
            assert_eq!(result.score, expected, "depth {depth}");
        }
    }

    #[test]
    fn search_finds_an_immediate_den_win() {
        let position = Position::from_piece_placements_checked(
            &[
                (10, Piece::new(Side::Red, PieceKind::Cat)),
                (62, Piece::new(Side::Blue, PieceKind::Elephant)),
            ],
            Side::Red,
        )
        .unwrap();
        let winning = position.find_legal_move(10, 3).unwrap();
        let mut engine = SearchEngine::new(SearchConfig::easy());
        let result = engine
            .choose_move(&position, SearchLimits::fixed_nodes(2_000), &NoopControl)
            .unwrap();
        assert_eq!(result.best_move, Some(winning));
        assert_eq!(result.termination, SearchTermination::Proven);
    }

    #[test]
    fn terminal_position_returns_no_move() {
        let position = Position::from_piece_placements_checked(
            &[
                (3, Piece::new(Side::Red, PieceKind::Cat)),
                (62, Piece::new(Side::Blue, PieceKind::Elephant)),
            ],
            Side::Blue,
        )
        .unwrap();
        let mut engine = SearchEngine::new(SearchConfig::easy());
        let result = engine
            .choose_move(&position, SearchLimits::fixed_nodes(100), &NoopControl)
            .unwrap();
        assert_eq!(result.best_move, None);
        assert_eq!(result.termination, SearchTermination::Terminal);
    }

    #[test]
    fn lane_configuration_is_stable_and_does_not_change_legality() {
        let position = Position::initial();
        let mut engine = SearchEngine::new(SearchConfig::easy());
        engine.configure_lane(3);
        let first = engine
            .choose_move(&position, SearchLimits::fixed_nodes(1_000), &NoopControl)
            .unwrap();
        engine.reset();
        let second = engine
            .choose_move(&position, SearchLimits::fixed_nodes(1_000), &NoopControl)
            .unwrap();
        assert_eq!(first.best_move, second.best_move);
        assert!(
            position
                .legal_moves()
                .iter()
                .any(|movement| Some(*movement) == first.best_move)
        );
    }

    #[derive(Debug)]
    struct DrawOracle;

    impl EndgameOracle for DrawOracle {
        fn probe(&self, position: &Position) -> Option<OracleHit> {
            (position.piece_count(Side::Blue) == 1 && position.piece_count(Side::Red) == 1)
                .then_some(OracleHit {
                    outcome: OracleOutcome::Draw,
                    distance: None,
                })
        }
    }

    #[test]
    fn exact_oracle_terminates_after_a_completed_root_iteration() {
        let position = Position::from_piece_placements_checked(
            &[
                (42, Piece::new(Side::Blue, PieceKind::Cat)),
                (20, Piece::new(Side::Red, PieceKind::Dog)),
            ],
            Side::Blue,
        )
        .unwrap();
        let mut engine = SearchEngine::with_oracle(SearchConfig::easy(), Arc::new(DrawOracle));
        let result = engine
            .choose_move(&position, SearchLimits::fixed_nodes(1_000), &NoopControl)
            .unwrap();
        assert_eq!(result.depth, 1);
        assert_eq!(result.termination, SearchTermination::Proven);
        assert!(result.tablebase_hits > 0);
    }
}
