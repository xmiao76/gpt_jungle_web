use serde_json::Error as JsonError;
use thiserror::Error;

use crate::{
    BOARD_SIZE, ENGINE_VERSION, GameStateDto, JungleSaveV1, Move, MoveList, Outcome, Piece,
    Position, PositionError, ResultType, SaveSettingsV1, Undo,
};

const MAX_IMPORTED_HISTORY: usize = 10_000;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("coordinate {0} is outside the canonical range 0..=62")]
    InvalidSquare(i32),
    #[error("the game is already over")]
    GameOver,
    #[error("move {origin}->{destination} is not legal in the current position")]
    IllegalMove { origin: i32, destination: i32 },
    #[error("invalid position: {0}")]
    InvalidPosition(#[from] PositionError),
    #[error("invalid game state: {0}")]
    InvalidState(String),
    #[error("invalid move history at ply {ply}: {message}")]
    InvalidHistory { ply: usize, message: String },
    #[error("unsupported save: {0}")]
    InvalidSave(String),
    #[error("invalid JSON: {0}")]
    Json(#[from] JsonError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AppliedPly {
    movement: Move,
    undo: Undo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    position: Position,
    history: Vec<Move>,
    /// Save v1 has no field for a nonstandard history origin. Histories are
    /// serialized only when they canonically replay from the initial setup;
    /// puzzle sessions retain their UI log but serialize as a strict
    /// historyless position.
    history_replayable_from_initial: bool,
    undo_stack: Vec<AppliedPly>,
    redo_stack: Vec<Move>,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    #[must_use]
    pub fn new() -> Self {
        Self {
            position: Position::initial(),
            history: Vec::new(),
            history_replayable_from_initial: true,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Creates a historyless session around a strictly validated puzzle position.
    pub fn from_position(position: Position) -> Self {
        let history_replayable_from_initial = position == Position::initial();
        Self {
            position,
            history: Vec::new(),
            history_replayable_from_initial,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    #[must_use]
    pub const fn position(&self) -> &Position {
        &self.position
    }

    #[must_use]
    pub fn history(&self) -> &[Move] {
        &self.history
    }

    #[must_use]
    pub fn legal_moves(&self) -> MoveList {
        self.position.legal_moves()
    }

    #[must_use]
    pub fn outcome(&self) -> Outcome {
        self.position.outcome()
    }

    #[must_use]
    pub fn effective_rank(&self, square: u8) -> u8 {
        self.position.effective_rank(square)
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    #[must_use]
    pub fn last_move(&self) -> Option<Move> {
        self.history.last().copied()
    }

    pub fn apply_coordinates(
        &mut self,
        origin: i32,
        destination: i32,
    ) -> Result<Move, SessionError> {
        let origin_u8 = checked_square(origin)?;
        let destination_u8 = checked_square(destination)?;
        if self.outcome().is_over() {
            return Err(SessionError::GameOver);
        }
        let movement = self
            .position
            .find_legal_move(origin_u8, destination_u8)
            .ok_or(SessionError::IllegalMove {
                origin,
                destination,
            })?;
        self.apply_canonical(movement);
        Ok(movement)
    }

    pub fn apply_move(&mut self, movement: Move) -> Result<Move, SessionError> {
        if self.outcome().is_over() {
            return Err(SessionError::GameOver);
        }
        let canonical = self
            .position
            .find_legal_move(movement.origin(), movement.destination())
            .ok_or(SessionError::IllegalMove {
                origin: movement.origin() as i32,
                destination: movement.destination() as i32,
            })?;
        if canonical != movement {
            return Err(SessionError::IllegalMove {
                origin: movement.origin() as i32,
                destination: movement.destination() as i32,
            });
        }
        self.apply_canonical(canonical);
        Ok(canonical)
    }

    fn apply_canonical(&mut self, movement: Move) {
        let undo = self.position.make_move_unchecked(movement);
        self.history.push(movement);
        self.undo_stack.push(AppliedPly { movement, undo });
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) -> bool {
        let Some(applied) = self.undo_stack.pop() else {
            return false;
        };
        debug_assert_eq!(self.history.last(), Some(&applied.movement));
        self.history.pop();
        self.position.unmake_move(applied.movement, applied.undo);
        self.redo_stack.push(applied.movement);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(movement) = self.redo_stack.pop() else {
            return false;
        };
        if self.outcome().is_over()
            || self
                .position
                .find_legal_move(movement.origin(), movement.destination())
                != Some(movement)
        {
            self.redo_stack.push(movement);
            return false;
        }
        let undo = self.position.make_move_unchecked(movement);
        self.history.push(movement);
        self.undo_stack.push(AppliedPly { movement, undo });
        true
    }

    #[must_use]
    pub fn game_state_dto(&self) -> GameStateDto {
        let outcome = self.outcome();
        GameStateDto {
            board: self
                .position
                .board_codes()
                .iter()
                .map(|&code| Piece::from_code(code))
                .collect(),
            side_to_move: self.position.side_to_move(),
            move_history: if self.history_replayable_from_initial {
                self.history.clone()
            } else {
                Vec::new()
            },
            winner: outcome.winner(),
            result: ResultType::from_outcome(outcome),
            result_reason: outcome.reason_text(),
        }
    }

    pub fn from_game_state_dto(state: GameStateDto) -> Result<Self, SessionError> {
        if state.board.len() != BOARD_SIZE {
            return Err(SessionError::InvalidState(format!(
                "board must contain exactly {BOARD_SIZE} squares"
            )));
        }
        if state.move_history.len() > MAX_IMPORTED_HISTORY {
            return Err(SessionError::InvalidState(format!(
                "move history exceeds {MAX_IMPORTED_HISTORY} plies"
            )));
        }

        let mut board = [0_u8; BOARD_SIZE];
        for (square, item) in state.board.iter().copied().enumerate() {
            board[square] = item.map_or(0, Piece::code);
        }
        let supplied_position = Position::from_board_codes_checked(board, state.side_to_move)?;

        let mut session = if state.move_history.is_empty() {
            Self::from_position(supplied_position)
        } else {
            let mut replay = Self::new();
            for (index, expected) in state.move_history.iter().copied().enumerate() {
                let actual = replay
                    .apply_coordinates(expected.origin() as i32, expected.destination() as i32)
                    .map_err(|error| SessionError::InvalidHistory {
                        ply: index + 1,
                        message: error.to_string(),
                    })?;
                if actual != expected {
                    return Err(SessionError::InvalidHistory {
                        ply: index + 1,
                        message: "piece, capture, or jump metadata disagrees with canonical replay"
                            .to_owned(),
                    });
                }
            }
            if replay.position != supplied_position {
                return Err(SessionError::InvalidState(
                    "board or side_to_move disagrees with replayed move history".to_owned(),
                ));
            }
            // Portable save v1 never preserved command stacks. Keep the full
            // display/history log while starting a fresh undo/redo boundary.
            replay.undo_stack.clear();
            replay.redo_stack.clear();
            replay
        };

        validate_declared_outcome(&session.position, &state)?;
        session.history = state.move_history;
        Ok(session)
    }

    pub fn to_game_state_json(&self) -> Result<String, SessionError> {
        Ok(serde_json::to_string(&self.game_state_dto())?)
    }

    pub fn to_game_state_json_pretty(&self) -> Result<String, SessionError> {
        Ok(serde_json::to_string_pretty(&self.game_state_dto())?)
    }

    pub fn from_game_state_json(value: &str) -> Result<Self, SessionError> {
        let state: GameStateDto = serde_json::from_str(value)?;
        Self::from_game_state_dto(state)
    }

    #[must_use]
    pub fn export_save_v1(
        &self,
        engine_version: impl Into<String>,
        created_at: impl Into<String>,
        settings: SaveSettingsV1,
    ) -> JungleSaveV1 {
        JungleSaveV1 {
            schema: JungleSaveV1::SCHEMA.to_owned(),
            version: JungleSaveV1::VERSION,
            engine_version: engine_version.into(),
            created_at: created_at.into(),
            game_state: self.game_state_dto(),
            settings,
        }
    }

    #[must_use]
    pub fn export_current_save_v1(
        &self,
        created_at: impl Into<String>,
        settings: SaveSettingsV1,
    ) -> JungleSaveV1 {
        self.export_save_v1(ENGINE_VERSION, created_at, settings)
    }

    pub fn export_save_v1_json(
        &self,
        engine_version: impl Into<String>,
        created_at: impl Into<String>,
        settings: SaveSettingsV1,
    ) -> Result<String, SessionError> {
        Ok(serde_json::to_string_pretty(&self.export_save_v1(
            engine_version,
            created_at,
            settings,
        ))?)
    }

    pub fn from_save_v1(save: &JungleSaveV1) -> Result<Self, SessionError> {
        validate_save_header(save)?;
        Self::from_game_state_dto(save.game_state.clone())
    }

    pub fn import_save_v1_json(value: &str) -> Result<(Self, JungleSaveV1), SessionError> {
        let save: JungleSaveV1 = serde_json::from_str(value)?;
        let session = Self::from_save_v1(&save)?;
        Ok((session, save))
    }
}

fn checked_square(value: i32) -> Result<u8, SessionError> {
    if !(0..BOARD_SIZE as i32).contains(&value) {
        Err(SessionError::InvalidSquare(value))
    } else {
        Ok(value as u8)
    }
}

fn validate_declared_outcome(
    position: &Position,
    state: &GameStateDto,
) -> Result<(), SessionError> {
    let actual = position.outcome();
    if state.winner != actual.winner() {
        return Err(SessionError::InvalidState(
            "winner disagrees with the canonical board outcome".to_owned(),
        ));
    }
    if state.result != ResultType::from_outcome(actual) {
        return Err(SessionError::InvalidState(
            "result disagrees with the canonical board outcome".to_owned(),
        ));
    }
    if state.result_reason != actual.reason_text() {
        return Err(SessionError::InvalidState(
            "result_reason disagrees with the canonical board outcome".to_owned(),
        ));
    }
    Ok(())
}

fn validate_save_header(save: &JungleSaveV1) -> Result<(), SessionError> {
    if save.schema != JungleSaveV1::SCHEMA {
        return Err(SessionError::InvalidSave("unrecognized schema".to_owned()));
    }
    if save.version != JungleSaveV1::VERSION {
        return Err(SessionError::InvalidSave(format!(
            "unsupported schema version {}",
            save.version
        )));
    }
    if save.engine_version.trim().is_empty() {
        return Err(SessionError::InvalidSave(
            "engineVersion must not be empty".to_owned(),
        ));
    }
    if save.created_at.trim().is_empty() {
        return Err(SessionError::InvalidSave(
            "createdAt must not be empty".to_owned(),
        ));
    }
    Ok(())
}
