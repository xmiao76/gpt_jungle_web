use core::fmt;

use serde::{Deserialize, Serialize};

use crate::BOARD_SIZE;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Blue = 0,
    Red = 1,
}

impl Side {
    #[must_use]
    pub const fn opponent(self) -> Self {
        match self {
            Self::Blue => Self::Red,
            Self::Red => Self::Blue,
        }
    }

    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Blue => "Blue",
            Self::Red => "Red",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PieceKind {
    Rat = 1,
    Cat = 2,
    Dog = 3,
    Wolf = 4,
    Leopard = 5,
    Tiger = 6,
    Lion = 7,
    Elephant = 8,
}

impl PieceKind {
    pub const ALL: [Self; 8] = [
        Self::Rat,
        Self::Cat,
        Self::Dog,
        Self::Wolf,
        Self::Leopard,
        Self::Tiger,
        Self::Lion,
        Self::Elephant,
    ];

    #[must_use]
    pub const fn rank(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub const fn from_rank(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Rat),
            2 => Some(Self::Cat),
            3 => Some(Self::Dog),
            4 => Some(Self::Wolf),
            5 => Some(Self::Leopard),
            6 => Some(Self::Tiger),
            7 => Some(Self::Lion),
            8 => Some(Self::Elephant),
            _ => None,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Rat => "Rat",
            Self::Cat => "Cat",
            Self::Dog => "Dog",
            Self::Wolf => "Wolf",
            Self::Leopard => "Leopard",
            Self::Tiger => "Tiger",
            Self::Lion => "Lion",
            Self::Elephant => "Elephant",
        }
    }
}

/// Compact piece code: 0 is empty, Blue is 1..=8, Red is 9..=16.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Piece(u8);

impl Piece {
    #[must_use]
    pub const fn new(side: Side, kind: PieceKind) -> Self {
        Self(side as u8 * 8 + kind as u8)
    }

    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        if code >= 1 && code <= 16 {
            Some(Self(code))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn code(self) -> u8 {
        self.0
    }

    #[must_use]
    pub const fn side(self) -> Side {
        if self.0 <= 8 { Side::Blue } else { Side::Red }
    }

    #[must_use]
    pub const fn kind(self) -> PieceKind {
        // Construction guarantees this is in 1..=8.
        match (self.0 - 1) % 8 + 1 {
            1 => PieceKind::Rat,
            2 => PieceKind::Cat,
            3 => PieceKind::Dog,
            4 => PieceKind::Wolf,
            5 => PieceKind::Leopard,
            6 => PieceKind::Tiger,
            7 => PieceKind::Lion,
            _ => PieceKind::Elephant,
        }
    }
}

impl fmt::Debug for Piece {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Piece")
            .field("side", &self.side())
            .field("kind", &self.kind())
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WinReason {
    DenEntry,
    CaptureAll,
    NoLegalMoves,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    Ongoing,
    Win { winner: Side, reason: WinReason },
}

impl Outcome {
    #[must_use]
    pub const fn is_over(self) -> bool {
        matches!(self, Self::Win { .. })
    }

    #[must_use]
    pub const fn winner(self) -> Option<Side> {
        match self {
            Self::Ongoing => None,
            Self::Win { winner, .. } => Some(winner),
        }
    }

    #[must_use]
    pub const fn win_reason(self) -> Option<WinReason> {
        match self {
            Self::Ongoing => None,
            Self::Win { reason, .. } => Some(reason),
        }
    }

    #[must_use]
    pub fn reason_text(self) -> String {
        match self {
            Self::Ongoing => String::new(),
            Self::Win {
                winner,
                reason: WinReason::DenEntry,
            } => {
                format!(
                    "{} entered the {} den.",
                    winner.title(),
                    winner.opponent().title().to_lowercase()
                )
            }
            Self::Win {
                winner,
                reason: WinReason::CaptureAll,
            } => format!(
                "{} captured all {} pieces.",
                winner.title(),
                winner.opponent().title().to_lowercase()
            ),
            Self::Win {
                winner: _,
                reason: WinReason::NoLegalMoves,
            } => "No legal moves remain.".to_owned(),
        }
    }
}

const ORIGIN_MASK: u32 = 0x3f;
const DESTINATION_SHIFT: u32 = 6;
const DESTINATION_MASK: u32 = 0x3f;
const CAPTURE_SHIFT: u32 = 12;
const CAPTURE_MASK: u32 = 0x1f;
const JUMP_SHIFT: u32 = 17;
const JUMP_MASK: u32 = 1 << JUMP_SHIFT;
const PIECE_SHIFT: u32 = 18;
const PIECE_MASK: u32 = 0x1f;
const USED_MOVE_MASK: u32 = ORIGIN_MASK
    | (DESTINATION_MASK << DESTINATION_SHIFT)
    | (CAPTURE_MASK << CAPTURE_SHIFT)
    | JUMP_MASK
    | (PIECE_MASK << PIECE_SHIFT);

/// Search-friendly move packed into 23 bits.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Move(u32);

impl Move {
    pub(crate) const EMPTY: Self = Self(0);

    #[must_use]
    pub const fn from_parts(
        origin: u8,
        destination: u8,
        piece: Piece,
        captured: Option<Piece>,
        is_jump: bool,
    ) -> Option<Self> {
        if origin as usize >= BOARD_SIZE
            || destination as usize >= BOARD_SIZE
            || origin == destination
        {
            return None;
        }
        if let Some(target) = captured
            && target.side() as u8 == piece.side() as u8
        {
            return None;
        }
        Some(Self(
            origin as u32
                | ((destination as u32) << DESTINATION_SHIFT)
                | ((match captured {
                    Some(value) => value.code(),
                    None => 0,
                } as u32)
                    << CAPTURE_SHIFT)
                | (if is_jump { JUMP_MASK } else { 0 })
                | ((piece.code() as u32) << PIECE_SHIFT),
        ))
    }

    #[must_use]
    pub const fn from_packed(packed: u32) -> Option<Self> {
        if packed & !USED_MOVE_MASK != 0 {
            return None;
        }
        let origin = (packed & ORIGIN_MASK) as u8;
        let destination = ((packed >> DESTINATION_SHIFT) & DESTINATION_MASK) as u8;
        let piece_code = ((packed >> PIECE_SHIFT) & PIECE_MASK) as u8;
        let captured_code = ((packed >> CAPTURE_SHIFT) & CAPTURE_MASK) as u8;
        let Some(piece) = Piece::from_code(piece_code) else {
            return None;
        };
        let captured = if captured_code == 0 {
            None
        } else {
            let Some(value) = Piece::from_code(captured_code) else {
                return None;
            };
            Some(value)
        };
        Self::from_parts(
            origin,
            destination,
            piece,
            captured,
            packed & JUMP_MASK != 0,
        )
    }

    #[must_use]
    pub const fn packed(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn origin(self) -> u8 {
        (self.0 & ORIGIN_MASK) as u8
    }

    #[must_use]
    pub const fn destination(self) -> u8 {
        ((self.0 >> DESTINATION_SHIFT) & DESTINATION_MASK) as u8
    }

    #[must_use]
    pub const fn piece(self) -> Piece {
        // Every public Move is validated at construction/deserialization.
        Piece(((self.0 >> PIECE_SHIFT) & PIECE_MASK) as u8)
    }

    #[must_use]
    pub const fn captured(self) -> Option<Piece> {
        Piece::from_code(((self.0 >> CAPTURE_SHIFT) & CAPTURE_MASK) as u8)
    }

    #[must_use]
    pub const fn is_jump(self) -> bool {
        self.0 & JUMP_MASK != 0
    }

    #[must_use]
    pub const fn note(self) -> &'static str {
        match (self.is_jump(), self.captured().is_some()) {
            (true, true) => "jump-capture",
            (true, false) => "jump",
            _ => "",
        }
    }
}

impl fmt::Debug for Move {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Move")
            .field("origin", &self.origin())
            .field("destination", &self.destination())
            .field("piece", &self.piece())
            .field("captured", &self.captured())
            .field("is_jump", &self.is_jump())
            .finish()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct MoveList {
    moves: [Move; Self::CAPACITY],
    len: u8,
}

impl MoveList {
    pub const CAPACITY: usize = 64;

    #[must_use]
    pub const fn new() -> Self {
        Self {
            moves: [Move::EMPTY; Self::CAPACITY],
            len: 0,
        }
    }

    pub(crate) fn push(&mut self, value: Move) {
        let index = self.len as usize;
        assert!(index < Self::CAPACITY, "legal move capacity exceeded");
        self.moves[index] = value;
        self.len += 1;
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.len as usize
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[must_use]
    pub fn as_slice(&self) -> &[Move] {
        &self.moves[..self.len()]
    }

    pub fn iter(&self) -> core::slice::Iter<'_, Move> {
        self.as_slice().iter()
    }
}

impl Default for MoveList {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for MoveList {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_list().entries(self.as_slice()).finish()
    }
}

impl<'a> IntoIterator for &'a MoveList {
    type Item = &'a Move;
    type IntoIter = core::slice::Iter<'a, Move>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
