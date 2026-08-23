use serde::{Deserialize, Deserializer, Serialize, Serializer, de, ser::SerializeStruct};

use crate::{BOARD_SIZE, Move, Outcome, Piece, PieceKind, Side, WinReason};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PieceSerde {
    side: Side,
    kind: PieceKind,
}

impl Serialize for Piece {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        PieceSerde {
            side: self.side(),
            kind: self.kind(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Piece {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = PieceSerde::deserialize(deserializer)?;
        Ok(Self::new(value.side, value.kind))
    }
}

impl Serialize for Move {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Move", 6)?;
        state.serialize_field("origin", &self.origin())?;
        state.serialize_field("destination", &self.destination())?;
        state.serialize_field("piece", &self.piece())?;
        state.serialize_field("captured", &self.captured())?;
        state.serialize_field("is_jump", &self.is_jump())?;
        state.serialize_field("note", self.note())?;
        state.end()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MoveSerde {
    origin: i32,
    destination: i32,
    piece: Piece,
    captured: Option<Piece>,
    is_jump: bool,
    note: String,
}

impl<'de> Deserialize<'de> for Move {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = MoveSerde::deserialize(deserializer)?;
        if !(0..BOARD_SIZE as i32).contains(&value.origin) {
            return Err(de::Error::custom(
                "move origin must be an integer from 0 to 62",
            ));
        }
        if !(0..BOARD_SIZE as i32).contains(&value.destination) {
            return Err(de::Error::custom(
                "move destination must be an integer from 0 to 62",
            ));
        }
        let movement = Move::from_parts(
            value.origin as u8,
            value.destination as u8,
            value.piece,
            value.captured,
            value.is_jump,
        )
        .ok_or_else(|| de::Error::custom("move fields are internally inconsistent"))?;
        if value.note != movement.note() {
            return Err(de::Error::custom(
                "move note does not match its capture/jump flags",
            ));
        }
        Ok(movement)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoordinatesDto {
    pub origin: i32,
    pub destination: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultType {
    Ongoing,
    DenEntry,
    CaptureAll,
    NoLegalMoves,
}

impl ResultType {
    #[must_use]
    pub const fn from_outcome(outcome: Outcome) -> Self {
        match outcome {
            Outcome::Ongoing => Self::Ongoing,
            Outcome::Win {
                reason: WinReason::DenEntry,
                ..
            } => Self::DenEntry,
            Outcome::Win {
                reason: WinReason::CaptureAll,
                ..
            } => Self::CaptureAll,
            Outcome::Win {
                reason: WinReason::NoLegalMoves,
                ..
            } => Self::NoLegalMoves,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameStateDto {
    pub board: Vec<Option<Piece>>,
    pub side_to_move: Side,
    #[serde(default)]
    pub move_history: Vec<Move>,
    pub winner: Option<Side>,
    pub result: ResultType,
    pub result_reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveSettingsV1 {
    pub difficulty: Difficulty,
    pub human_side: Side,
    pub board_flipped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JungleSaveV1 {
    pub schema: String,
    pub version: u32,
    pub engine_version: String,
    pub created_at: String,
    pub game_state: GameStateDto,
    pub settings: SaveSettingsV1,
}

impl JungleSaveV1 {
    pub const SCHEMA: &'static str = "gpt-jungle-save";
    pub const VERSION: u32 = 1;
}
