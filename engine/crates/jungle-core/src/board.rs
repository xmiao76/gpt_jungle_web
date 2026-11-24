use serde::{Deserialize, Serialize};

use crate::{PieceKind, Side};

pub const BOARD_COLS: usize = 7;
pub const BOARD_ROWS: usize = 9;
pub const BOARD_SIZE: usize = BOARD_COLS * BOARD_ROWS;

pub const RED_DEN: u8 = 59;
pub const BLUE_DEN: u8 = 3;
pub const RED_TRAPS: [u8; 3] = [52, 58, 60];
pub const BLUE_TRAPS: [u8; 3] = [2, 4, 10];
pub const WATER: [u8; 12] = [22, 23, 25, 26, 29, 30, 32, 33, 36, 37, 39, 40];
pub const WATER_MASK: u64 = mask_of(&WATER);
pub const RED_TRAP_MASK: u64 = mask_of(&RED_TRAPS);
pub const BLUE_TRAP_MASK: u64 = mask_of(&BLUE_TRAPS);

const fn mask_of(values: &[u8]) -> u64 {
    let mut result = 0_u64;
    let mut index = 0;
    while index < values.len() {
        result |= 1_u64 << values[index];
        index += 1;
    }
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Terrain {
    Land,
    Water,
    Trap,
    Den,
}

#[must_use]
pub const fn in_bounds_i32(value: i32) -> bool {
    value >= 0 && value < BOARD_SIZE as i32
}

#[must_use]
pub const fn own_den(side: Side) -> u8 {
    match side {
        Side::Blue => BLUE_DEN,
        Side::Red => RED_DEN,
    }
}

#[must_use]
pub const fn opponent_den(side: Side) -> u8 {
    own_den(side.opponent())
}

#[must_use]
pub const fn trap_owner(square: u8) -> Option<Side> {
    let bit = 1_u64 << square;
    if RED_TRAP_MASK & bit != 0 {
        Some(Side::Red)
    } else if BLUE_TRAP_MASK & bit != 0 {
        Some(Side::Blue)
    } else {
        None
    }
}

#[must_use]
pub const fn is_water(square: u8) -> bool {
    WATER_MASK & (1_u64 << square) != 0
}

#[must_use]
pub const fn terrain_at(square: u8) -> Terrain {
    if is_water(square) {
        Terrain::Water
    } else if square == RED_DEN || square == BLUE_DEN {
        Terrain::Den
    } else if trap_owner(square).is_some() {
        Terrain::Trap
    } else {
        Terrain::Land
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SquareList {
    squares: [u8; 4],
    len: u8,
}

impl SquareList {
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.squares[..self.len as usize]
    }

    pub fn iter(&self) -> core::slice::Iter<'_, u8> {
        self.as_slice().iter()
    }
}

const EMPTY_SQUARE_LIST: SquareList = SquareList {
    squares: [0; 4],
    len: 0,
};
const DIRECTIONS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

const fn build_neighbors() -> [SquareList; BOARD_SIZE] {
    let mut table = [EMPTY_SQUARE_LIST; BOARD_SIZE];
    let mut square = 0;
    while square < BOARD_SIZE {
        let row = square as i32 / BOARD_COLS as i32;
        let col = square as i32 % BOARD_COLS as i32;
        let mut direction = 0;
        while direction < DIRECTIONS.len() {
            let next_row = row + DIRECTIONS[direction].0;
            let next_col = col + DIRECTIONS[direction].1;
            if next_row >= 0
                && next_row < BOARD_ROWS as i32
                && next_col >= 0
                && next_col < BOARD_COLS as i32
            {
                let len = table[square].len as usize;
                table[square].squares[len] = (next_row * BOARD_COLS as i32 + next_col) as u8;
                table[square].len += 1;
            }
            direction += 1;
        }
        square += 1;
    }
    table
}

pub const NEIGHBORS: [SquareList; BOARD_SIZE] = build_neighbors();

#[must_use]
pub const fn neighbors(square: u8) -> SquareList {
    debug_assert!((square as usize) < BOARD_SIZE);
    NEIGHBORS[square as usize]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RiverJump {
    pub destination: u8,
    pub path_mask: u64,
    pub water_span: u8,
}

impl RiverJump {
    #[must_use]
    pub const fn allowed_for(self, kind: PieceKind) -> bool {
        match kind {
            PieceKind::Lion => true,
            PieceKind::Tiger => self.water_span == 2,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JumpList {
    jumps: [RiverJump; 4],
    len: u8,
}

impl JumpList {
    #[must_use]
    pub fn as_slice(&self) -> &[RiverJump] {
        &self.jumps[..self.len as usize]
    }

    pub fn iter(&self) -> core::slice::Iter<'_, RiverJump> {
        self.as_slice().iter()
    }
}

const EMPTY_RIVER_JUMP: RiverJump = RiverJump {
    destination: 0,
    path_mask: 0,
    water_span: 0,
};
const EMPTY_JUMP_LIST: JumpList = JumpList {
    jumps: [EMPTY_RIVER_JUMP; 4],
    len: 0,
};

const fn build_river_jumps() -> [JumpList; BOARD_SIZE] {
    let mut table = [EMPTY_JUMP_LIST; BOARD_SIZE];
    let mut square = 0;
    while square < BOARD_SIZE {
        let row = square as i32 / BOARD_COLS as i32;
        let col = square as i32 % BOARD_COLS as i32;
        let mut direction = 0;
        while direction < DIRECTIONS.len() {
            let dr = DIRECTIONS[direction].0;
            let dc = DIRECTIONS[direction].1;
            let mut next_row = row + dr;
            let mut next_col = col + dc;
            if next_row >= 0
                && next_row < BOARD_ROWS as i32
                && next_col >= 0
                && next_col < BOARD_COLS as i32
            {
                let first = (next_row * BOARD_COLS as i32 + next_col) as u8;
                if is_water(first) {
                    let mut path_mask = 0_u64;
                    let mut water_span = 0_u8;
                    while next_row >= 0
                        && next_row < BOARD_ROWS as i32
                        && next_col >= 0
                        && next_col < BOARD_COLS as i32
                    {
                        let next = (next_row * BOARD_COLS as i32 + next_col) as u8;
                        if !is_water(next) {
                            break;
                        }
                        path_mask |= 1_u64 << next;
                        water_span += 1;
                        next_row += dr;
                        next_col += dc;
                    }
                    if next_row >= 0
                        && next_row < BOARD_ROWS as i32
                        && next_col >= 0
                        && next_col < BOARD_COLS as i32
                    {
                        let len = table[square].len as usize;
                        table[square].jumps[len] = RiverJump {
                            destination: (next_row * BOARD_COLS as i32 + next_col) as u8,
                            path_mask,
                            water_span,
                        };
                        table[square].len += 1;
                    }
                }
            }
            direction += 1;
        }
        square += 1;
    }
    table
}

pub const RIVER_JUMPS: [JumpList; BOARD_SIZE] = build_river_jumps();

#[must_use]
pub const fn river_jumps(square: u8) -> JumpList {
    debug_assert!((square as usize) < BOARD_SIZE);
    RIVER_JUMPS[square as usize]
}
