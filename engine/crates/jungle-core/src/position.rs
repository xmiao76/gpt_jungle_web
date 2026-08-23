use thiserror::Error;

use crate::{
    BLUE_DEN, BLUE_TRAP_MASK, BOARD_SIZE, Move, MoveList, Piece, PieceKind, RED_DEN, RED_TRAP_MASK,
    Side, WATER_MASK, WinReason, is_water, neighbors, own_den, river_jumps, trap_owner,
};

const PIECE_CODE_COUNT: usize = 17;
const NO_PIECE: u8 = 0;
const MASK_64: u64 = u64::MAX;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PositionError {
    #[error("square {0} is outside the canonical range 0..=62")]
    InvalidSquare(i32),
    #[error("board square {square} contains invalid piece code {code}")]
    InvalidPieceCode { square: u8, code: u8 },
    #[error("more than one piece occupies square {0}")]
    SquareOccupied(u8),
    #[error("{side:?} contains more than one {kind:?}")]
    DuplicatePiece { side: Side, kind: PieceKind },
    #[error("only rats may occupy water square {0}")]
    NonRatInWater(u8),
    #[error("a {0:?} piece may not occupy its own den")]
    OwnDenOccupied(Side),
    #[error("both sides cannot occupy the opposing den")]
    BothDensWon,
    #[error("a position cannot contain zero pieces for both sides")]
    EmptyPosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum MoveError {
    #[error("game is already over")]
    GameOver,
    #[error("move encoding is not legal in the current position")]
    IllegalMove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Undo {
    captured: Option<Piece>,
}

impl Undo {
    #[must_use]
    pub const fn captured(self) -> Option<Piece> {
        self.captured
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Position {
    board: [u8; BOARD_SIZE],
    piece_squares: [u64; PIECE_CODE_COUNT],
    side_occupancy: [u64; 2],
    side_to_move: Side,
    zobrist_hash: u64,
}

impl core::fmt::Debug for Position {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("Position")
            .field("side_to_move", &self.side_to_move)
            .field("zobrist_hash", &format_args!("{:#018x}", self.zobrist_hash))
            .field("board", &self.board)
            .finish()
    }
}

impl Default for Position {
    fn default() -> Self {
        Self::initial()
    }
}

impl Position {
    #[must_use]
    pub fn initial() -> Self {
        let placements = [
            (42, Piece::new(Side::Blue, PieceKind::Elephant)),
            (44, Piece::new(Side::Blue, PieceKind::Wolf)),
            (46, Piece::new(Side::Blue, PieceKind::Leopard)),
            (48, Piece::new(Side::Blue, PieceKind::Rat)),
            (50, Piece::new(Side::Blue, PieceKind::Cat)),
            (54, Piece::new(Side::Blue, PieceKind::Dog)),
            (56, Piece::new(Side::Blue, PieceKind::Tiger)),
            (62, Piece::new(Side::Blue, PieceKind::Lion)),
            (14, Piece::new(Side::Red, PieceKind::Rat)),
            (16, Piece::new(Side::Red, PieceKind::Leopard)),
            (18, Piece::new(Side::Red, PieceKind::Wolf)),
            (20, Piece::new(Side::Red, PieceKind::Elephant)),
            (8, Piece::new(Side::Red, PieceKind::Dog)),
            (12, Piece::new(Side::Red, PieceKind::Cat)),
            (0, Piece::new(Side::Red, PieceKind::Lion)),
            (6, Piece::new(Side::Red, PieceKind::Tiger)),
        ];
        // The compiled initial position is necessarily valid.
        Self::from_piece_placements_checked(&placements, Side::Blue)
            .expect("compiled initial Jungle position must be valid")
    }

    pub fn from_piece_placements_checked(
        placements: &[(u8, Piece)],
        side_to_move: Side,
    ) -> Result<Self, PositionError> {
        let mut board = [NO_PIECE; BOARD_SIZE];
        for &(square, piece) in placements {
            if square as usize >= BOARD_SIZE {
                return Err(PositionError::InvalidSquare(square as i32));
            }
            if board[square as usize] != NO_PIECE {
                return Err(PositionError::SquareOccupied(square));
            }
            board[square as usize] = piece.code();
        }
        Self::from_board_codes_checked(board, side_to_move)
    }

    pub fn from_board_codes_checked(
        board: [u8; BOARD_SIZE],
        side_to_move: Side,
    ) -> Result<Self, PositionError> {
        let mut piece_squares = [0_u64; PIECE_CODE_COUNT];
        let mut side_occupancy = [0_u64; 2];
        let mut seen = [false; PIECE_CODE_COUNT];
        let mut zobrist_hash = if side_to_move == Side::Red {
            ZOBRIST_SIDE
        } else {
            0
        };

        for (square, &code) in board.iter().enumerate() {
            if code == NO_PIECE {
                continue;
            }
            let square_u8 = square as u8;
            let Some(piece) = Piece::from_code(code) else {
                return Err(PositionError::InvalidPieceCode {
                    square: square_u8,
                    code,
                });
            };
            if seen[code as usize] {
                return Err(PositionError::DuplicatePiece {
                    side: piece.side(),
                    kind: piece.kind(),
                });
            }
            seen[code as usize] = true;
            if is_water(square_u8) && piece.kind() != PieceKind::Rat {
                return Err(PositionError::NonRatInWater(square_u8));
            }
            if square_u8 == own_den(piece.side()) {
                return Err(PositionError::OwnDenOccupied(piece.side()));
            }
            let bit = 1_u64 << square;
            piece_squares[code as usize] |= bit;
            side_occupancy[piece.side().index()] |= bit;
            zobrist_hash ^= ZOBRIST_PIECES[square][code as usize];
        }

        if side_occupancy[0] == 0 && side_occupancy[1] == 0 {
            return Err(PositionError::EmptyPosition);
        }
        let blue_in_red_den = board[RED_DEN as usize]
            .and_then_piece()
            .is_some_and(|piece| piece.side() == Side::Blue);
        let red_in_blue_den = board[BLUE_DEN as usize]
            .and_then_piece()
            .is_some_and(|piece| piece.side() == Side::Red);
        if blue_in_red_den && red_in_blue_den {
            return Err(PositionError::BothDensWon);
        }

        Ok(Self {
            board,
            piece_squares,
            side_occupancy,
            side_to_move,
            zobrist_hash: zobrist_hash & MASK_64,
        })
    }

    #[must_use]
    pub const fn side_to_move(&self) -> Side {
        self.side_to_move
    }

    #[must_use]
    pub const fn board_codes(&self) -> &[u8; BOARD_SIZE] {
        &self.board
    }

    #[must_use]
    pub fn piece_at(&self, square: u8) -> Option<Piece> {
        self.board
            .get(square as usize)
            .copied()
            .and_then(Piece::from_code)
    }

    #[must_use]
    pub const fn piece_bitboard(&self, piece: Piece) -> u64 {
        self.piece_squares[piece.code() as usize]
    }

    #[must_use]
    pub const fn occupancy(&self, side: Side) -> u64 {
        self.side_occupancy[side.index()]
    }

    #[must_use]
    pub const fn all_occupancy(&self) -> u64 {
        self.side_occupancy[0] | self.side_occupancy[1]
    }

    #[must_use]
    pub fn piece_count(&self, side: Side) -> u32 {
        self.occupancy(side).count_ones()
    }

    #[must_use]
    pub const fn hash(&self) -> u64 {
        self.zobrist_hash
    }

    #[must_use]
    pub const fn zobrist_hash(&self) -> u64 {
        self.zobrist_hash
    }

    #[must_use]
    pub fn recompute_zobrist(&self) -> u64 {
        let mut value = if self.side_to_move == Side::Red {
            ZOBRIST_SIDE
        } else {
            0
        };
        for (square, &piece) in self.board.iter().enumerate() {
            if piece != NO_PIECE {
                value ^= ZOBRIST_PIECES[square][piece as usize];
            }
        }
        value
    }

    #[must_use]
    pub fn with_side_to_move(&self, side: Side) -> Self {
        let mut copy = self.clone();
        if copy.side_to_move != side {
            copy.side_to_move = side;
            copy.zobrist_hash ^= ZOBRIST_SIDE;
        }
        copy
    }

    #[must_use]
    pub fn effective_rank(&self, square: u8) -> u8 {
        let Some(piece) = self.piece_at(square) else {
            return 0;
        };
        if trap_owner(square).is_some_and(|owner| owner == piece.side().opponent()) {
            0
        } else {
            piece.kind().rank()
        }
    }

    #[must_use]
    pub fn can_capture(&self, attacker_square: u8, defender_square: u8) -> bool {
        if attacker_square as usize >= BOARD_SIZE || defender_square as usize >= BOARD_SIZE {
            return false;
        }
        let Some(attacker) = self.piece_at(attacker_square) else {
            return false;
        };
        let Some(defender) = self.piece_at(defender_square) else {
            return false;
        };
        if attacker.side() == defender.side() {
            return false;
        }

        let attacker_in_water = is_water(attacker_square);
        let defender_in_water = is_water(defender_square);
        if attacker.kind() == PieceKind::Rat && defender.kind() == PieceKind::Elephant {
            return !attacker_in_water && !defender_in_water;
        }
        if attacker.kind() == PieceKind::Elephant && defender.kind() == PieceKind::Rat {
            return self.effective_rank(defender_square) == 0;
        }
        if attacker_in_water || defender_in_water {
            return attacker_in_water
                && defender_in_water
                && attacker.kind() == PieceKind::Rat
                && defender.kind() == PieceKind::Rat;
        }
        self.effective_rank(attacker_square) >= self.effective_rank(defender_square)
    }

    #[must_use]
    pub fn legal_moves(&self) -> MoveList {
        if self.structural_outcome().is_some() {
            MoveList::new()
        } else {
            self.generate_moves_for(self.side_to_move)
        }
    }

    #[must_use]
    pub fn legal_moves_for(&self, side: Side) -> MoveList {
        if self.structural_outcome().is_some() {
            MoveList::new()
        } else {
            self.generate_moves_for(side)
        }
    }

    #[must_use]
    pub fn moves_from(&self, origin: u8) -> MoveList {
        if origin as usize >= BOARD_SIZE || self.structural_outcome().is_some() {
            return MoveList::new();
        }
        let Some(piece) = self.piece_at(origin) else {
            return MoveList::new();
        };
        if piece.side() != self.side_to_move {
            return MoveList::new();
        }
        let rat_mask = self.piece_bitboard(Piece::new(Side::Blue, PieceKind::Rat))
            | self.piece_bitboard(Piece::new(Side::Red, PieceKind::Rat));
        let mut result = MoveList::new();
        self.generate_piece_moves(origin, piece, rat_mask, &mut result);
        result
    }

    fn generate_moves_for(&self, side: Side) -> MoveList {
        let mut result = MoveList::new();
        let rat_mask = self.piece_bitboard(Piece::new(Side::Blue, PieceKind::Rat))
            | self.piece_bitboard(Piece::new(Side::Red, PieceKind::Rat));
        let mut occupancy = self.occupancy(side);
        while occupancy != 0 {
            let origin = occupancy.trailing_zeros() as u8;
            occupancy &= occupancy - 1;
            let piece = self
                .piece_at(origin)
                .expect("occupancy and board stay synchronized");
            self.generate_piece_moves(origin, piece, rat_mask, &mut result);
        }
        result
    }

    fn generate_piece_moves(&self, origin: u8, piece: Piece, rat_mask: u64, result: &mut MoveList) {
        for &destination in neighbors(origin).as_slice() {
            if destination == own_den(piece.side()) {
                continue;
            }
            if is_water(destination) && piece.kind() != PieceKind::Rat {
                continue;
            }
            match self.piece_at(destination) {
                None => result.push(
                    Move::from_parts(origin, destination, piece, None, false)
                        .expect("generated step is valid"),
                ),
                Some(target)
                    if target.side() != piece.side() && self.can_capture(origin, destination) =>
                {
                    result.push(
                        Move::from_parts(origin, destination, piece, Some(target), false)
                            .expect("generated capture is valid"),
                    );
                }
                Some(_) => {}
            }
        }

        if !matches!(piece.kind(), PieceKind::Lion | PieceKind::Tiger) {
            return;
        }
        for &jump in river_jumps(origin).as_slice() {
            if !jump.allowed_for(piece.kind()) || jump.path_mask & rat_mask != 0 {
                continue;
            }
            let destination = jump.destination;
            if destination == own_den(piece.side()) {
                continue;
            }
            match self.piece_at(destination) {
                None => result.push(
                    Move::from_parts(origin, destination, piece, None, true)
                        .expect("generated jump is valid"),
                ),
                Some(target)
                    if target.side() != piece.side() && self.can_capture(origin, destination) =>
                {
                    result.push(
                        Move::from_parts(origin, destination, piece, Some(target), true)
                            .expect("generated jump capture is valid"),
                    );
                }
                Some(_) => {}
            }
        }
    }

    #[must_use]
    pub fn find_legal_move(&self, origin: u8, destination: u8) -> Option<Move> {
        if origin as usize >= BOARD_SIZE || destination as usize >= BOARD_SIZE {
            return None;
        }
        self.moves_from(origin)
            .iter()
            .copied()
            .find(|candidate| candidate.destination() == destination)
    }

    pub fn try_make_move(&mut self, movement: Move) -> Result<Undo, MoveError> {
        if self.outcome().is_over() {
            return Err(MoveError::GameOver);
        }
        if !self
            .legal_moves()
            .iter()
            .any(|candidate| *candidate == movement)
        {
            return Err(MoveError::IllegalMove);
        }
        Ok(self.make_move_unchecked(movement))
    }

    pub fn make_move_unchecked(&mut self, movement: Move) -> Undo {
        let origin = movement.origin();
        let destination = movement.destination();
        let piece = movement.piece();
        let captured = movement.captured();
        debug_assert_eq!(self.piece_at(origin), Some(piece));
        debug_assert_eq!(self.piece_at(destination), captured);
        debug_assert_eq!(piece.side(), self.side_to_move);

        let origin_bit = 1_u64 << origin;
        let destination_bit = 1_u64 << destination;
        self.zobrist_hash ^= ZOBRIST_PIECES[origin as usize][piece.code() as usize];
        self.zobrist_hash ^= ZOBRIST_PIECES[destination as usize][piece.code() as usize];
        if let Some(target) = captured {
            self.zobrist_hash ^= ZOBRIST_PIECES[destination as usize][target.code() as usize];
            self.piece_squares[target.code() as usize] ^= destination_bit;
            self.side_occupancy[target.side().index()] ^= destination_bit;
        }
        self.zobrist_hash ^= ZOBRIST_SIDE;

        self.board[origin as usize] = NO_PIECE;
        self.board[destination as usize] = piece.code();
        self.piece_squares[piece.code() as usize] ^= origin_bit | destination_bit;
        self.side_occupancy[piece.side().index()] ^= origin_bit | destination_bit;
        self.side_to_move = self.side_to_move.opponent();
        Undo { captured }
    }

    pub fn unmake_move(&mut self, movement: Move, undo: Undo) {
        debug_assert_eq!(movement.captured(), undo.captured);
        let origin = movement.origin();
        let destination = movement.destination();
        let piece = movement.piece();
        let origin_bit = 1_u64 << origin;
        let destination_bit = 1_u64 << destination;

        self.side_to_move = self.side_to_move.opponent();
        self.zobrist_hash ^= ZOBRIST_SIDE;
        self.zobrist_hash ^= ZOBRIST_PIECES[origin as usize][piece.code() as usize];
        self.zobrist_hash ^= ZOBRIST_PIECES[destination as usize][piece.code() as usize];
        if let Some(target) = undo.captured {
            self.zobrist_hash ^= ZOBRIST_PIECES[destination as usize][target.code() as usize];
            self.piece_squares[target.code() as usize] ^= destination_bit;
            self.side_occupancy[target.side().index()] ^= destination_bit;
        }

        self.board[origin as usize] = piece.code();
        self.board[destination as usize] = undo.captured.map_or(NO_PIECE, Piece::code);
        self.piece_squares[piece.code() as usize] ^= origin_bit | destination_bit;
        self.side_occupancy[piece.side().index()] ^= origin_bit | destination_bit;
    }

    #[must_use]
    pub fn outcome(&self) -> crate::Outcome {
        if let Some(outcome) = self.structural_outcome() {
            return outcome;
        }
        let moves = self.generate_moves_for(self.side_to_move);
        self.outcome_from_legal_moves(&moves)
    }

    /// Derive the terminal result while reusing an already generated current-
    /// side move list. Search should prefer this over [`Self::outcome`] to avoid
    /// generating the same moves twice.
    #[must_use]
    pub fn outcome_from_legal_moves(&self, moves: &MoveList) -> crate::Outcome {
        if let Some(outcome) = self.structural_outcome() {
            return outcome;
        }
        if moves.is_empty() {
            crate::Outcome::Win {
                winner: self.side_to_move.opponent(),
                reason: WinReason::NoLegalMoves,
            }
        } else {
            crate::Outcome::Ongoing
        }
    }

    fn structural_outcome(&self) -> Option<crate::Outcome> {
        if self
            .piece_at(RED_DEN)
            .is_some_and(|piece| piece.side() == Side::Blue)
        {
            return Some(crate::Outcome::Win {
                winner: Side::Blue,
                reason: WinReason::DenEntry,
            });
        }
        if self
            .piece_at(BLUE_DEN)
            .is_some_and(|piece| piece.side() == Side::Red)
        {
            return Some(crate::Outcome::Win {
                winner: Side::Red,
                reason: WinReason::DenEntry,
            });
        }
        if self.occupancy(Side::Blue) == 0 {
            return Some(crate::Outcome::Win {
                winner: Side::Red,
                reason: WinReason::CaptureAll,
            });
        }
        if self.occupancy(Side::Red) == 0 {
            return Some(crate::Outcome::Win {
                winner: Side::Blue,
                reason: WinReason::CaptureAll,
            });
        }
        None
    }

    #[must_use]
    pub fn is_consistent(&self) -> bool {
        if self.zobrist_hash != self.recompute_zobrist() {
            return false;
        }
        let mut piece_squares = [0_u64; PIECE_CODE_COUNT];
        let mut side_occupancy = [0_u64; 2];
        for (square, &code) in self.board.iter().enumerate() {
            if let Some(piece) = Piece::from_code(code) {
                let bit = 1_u64 << square;
                piece_squares[code as usize] |= bit;
                side_occupancy[piece.side().index()] |= bit;
            } else if code != NO_PIECE {
                return false;
            }
        }
        piece_squares == self.piece_squares && side_occupancy == self.side_occupancy
    }
}

trait PieceCodeExt {
    fn and_then_piece(self) -> Option<Piece>;
}

impl PieceCodeExt for u8 {
    fn and_then_piece(self) -> Option<Piece> {
        Piece::from_code(self)
    }
}

const fn splitmix64(value: u64) -> (u64, u64) {
    let value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut mixed = value;
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    (value, mixed ^ (mixed >> 31))
}

const fn build_zobrist() -> ([[u64; PIECE_CODE_COUNT]; BOARD_SIZE], u64) {
    let mut seed = 0x4a55_4e47_4c45_5f31;
    let mut pieces = [[0_u64; PIECE_CODE_COUNT]; BOARD_SIZE];
    let mut square = 0;
    while square < BOARD_SIZE {
        let mut code = 1;
        while code < PIECE_CODE_COUNT {
            let result = splitmix64(seed);
            seed = result.0;
            pieces[square][code] = result.1;
            code += 1;
        }
        square += 1;
    }
    let result = splitmix64(seed);
    (pieces, result.1)
}

const ZOBRIST: ([[u64; PIECE_CODE_COUNT]; BOARD_SIZE], u64) = build_zobrist();
pub const ZOBRIST_PIECES: [[u64; PIECE_CODE_COUNT]; BOARD_SIZE] = ZOBRIST.0;
pub const ZOBRIST_SIDE: u64 = ZOBRIST.1;

// These masks are part of the stable evaluator/search contract.
pub const TARGET_TRAP_MASKS: [u64; 2] = [RED_TRAP_MASK, BLUE_TRAP_MASK];
pub const OWN_TRAP_MASKS: [u64; 2] = [BLUE_TRAP_MASK, RED_TRAP_MASK];
pub const TARGET_DENS: [u8; 2] = [RED_DEN, BLUE_DEN];
pub const OWN_DENS: [u8; 2] = [BLUE_DEN, RED_DEN];
pub const SWIMMABLE_MASK: u64 = WATER_MASK;
