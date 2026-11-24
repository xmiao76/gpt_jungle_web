//! Deterministic evaluation implementations.

use jungle_core::{
    BOARD_SIZE, PieceKind, Position, Side, WATER_MASK, neighbors, opponent_den, own_den,
    river_jumps, trap_owner,
};
use thiserror::Error;

const PIECE_VALUES: [i32; 9] = [0, 145, 235, 320, 405, 510, 755, 825, 900];

pub trait Evaluator: Send + Sync {
    /// Return a score from the side-to-move's perspective.
    fn evaluate(&self, position: &Position) -> i32;

    fn id(&self) -> &'static str;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ClassicalEvaluator;

impl Evaluator for ClassicalEvaluator {
    fn evaluate(&self, position: &Position) -> i32 {
        let absolute = evaluate_absolute(position);
        if position.side_to_move() == Side::Blue {
            absolute
        } else {
            -absolute
        }
    }

    fn id(&self) -> &'static str {
        "classical-v4"
    }
}

fn evaluate_absolute(position: &Position) -> i32 {
    let mut scores = [0_i32; 2];
    let mut closest = [24_u8; 2];
    let total_pieces = position.piece_count(Side::Blue) + position.piece_count(Side::Red);
    let phase = 16_i32.saturating_sub(total_pieces as i32);
    let rat_mask = position.piece_bitboard(jungle_core::Piece::new(Side::Blue, PieceKind::Rat))
        | position.piece_bitboard(jungle_core::Piece::new(Side::Red, PieceKind::Rat));
    let elephants = [
        position.piece_bitboard(jungle_core::Piece::new(Side::Blue, PieceKind::Elephant)) != 0,
        position.piece_bitboard(jungle_core::Piece::new(Side::Red, PieceKind::Elephant)) != 0,
    ];

    for square in 0..BOARD_SIZE as u8 {
        let Some(piece) = position.piece_at(square) else {
            continue;
        };
        let side = piece.side();
        let side_index = side.index();
        let kind = piece.kind();
        let kind_index = kind.rank() as usize;

        let mut material = PIECE_VALUES[kind_index];
        if trap_owner(square) == Some(side.opponent()) {
            material /= 3;
            scores[side_index] += 310;
        } else if trap_owner(square) == Some(side) {
            scores[side_index] += 55;
        }
        scores[side_index] += material;

        let distance = manhattan(square, opponent_den(side));
        closest[side_index] = closest[side_index].min(distance);
        scores[side_index] += (18 - i32::from(distance)) * 15;
        if distance <= 3 {
            scores[side_index] += (4 - i32::from(distance)) * (250 + phase * 22);
        }

        let home_distance = manhattan(square, own_den(side));
        if home_distance <= 2 {
            scores[side_index] += (3 - i32::from(home_distance)) * 65;
        }

        if kind == PieceKind::Rat {
            if WATER_MASK & (1_u64 << square) != 0 {
                scores[side_index] += 130;
            }
            if elephants[side.opponent().index()] {
                scores[side_index] += 75;
            }
        } else if matches!(kind, PieceKind::Tiger | PieceKind::Lion) {
            if (2..=6).contains(&(square / 7)) {
                scores[side_index] += 55;
            }
            let open_lanes = river_jumps(square)
                .iter()
                .filter(|jump| jump.allowed_for(kind) && jump.path_mask & rat_mask == 0)
                .count() as i32;
            scores[side_index] += open_lanes * 8;
        }
    }

    let blue_moves = position.legal_moves_for(Side::Blue);
    let red_moves = position.legal_moves_for(Side::Red);
    let moves = [&blue_moves, &red_moves];
    let defended = [
        defended_mask(position, Side::Blue, rat_mask),
        defended_mask(position, Side::Red, rat_mask),
    ];
    for side in [Side::Blue, Side::Red] {
        let us = side.index();
        scores[us] += moves[us].len() as i32 * (7 + phase / 3);
        scores[us] += (14 - i32::from(closest[us])) * (18 + phase * 4);
        for movement in moves[us] {
            if let Some(victim) = movement.captured() {
                let bit = 1_u64 << movement.destination();
                let divisor = if defended[side.opponent().index()] & bit != 0 {
                    12
                } else {
                    4
                };
                scores[us] += PIECE_VALUES[victim.kind().rank() as usize] / divisor;
            }
        }
        let enemy_close = closest[side.opponent().index()];
        if enemy_close <= 3 {
            scores[us] -= (4 - i32::from(enemy_close)) * (300 + phase * 25);
        }
    }

    let tempo = if position.side_to_move() == Side::Blue {
        12
    } else {
        -12
    };
    (scores[0] - scores[1] + tempo).clamp(-45_000, 45_000)
}

fn defended_mask(position: &Position, side: Side, rat_mask: u64) -> u64 {
    let mut defended = 0_u64;
    for square in 0..BOARD_SIZE as u8 {
        let Some(piece) = position
            .piece_at(square)
            .filter(|piece| piece.side() == side)
        else {
            continue;
        };
        for adjacent in neighbors(square).iter() {
            if position
                .piece_at(*adjacent)
                .is_some_and(|target| target.side() == side)
            {
                defended |= 1_u64 << *adjacent;
            }
        }
        if matches!(piece.kind(), PieceKind::Tiger | PieceKind::Lion) {
            for jump in river_jumps(square).iter() {
                if jump.allowed_for(piece.kind())
                    && jump.path_mask & rat_mask == 0
                    && position
                        .piece_at(jump.destination)
                        .is_some_and(|target| target.side() == side)
                {
                    defended |= 1_u64 << jump.destination;
                }
            }
        }
    }
    defended
}

#[inline]
fn manhattan(first: u8, second: u8) -> u8 {
    let (first_row, first_col) = (first / 7, first % 7);
    let (second_row, second_col) = (second / 7, second % 7);
    first_row.abs_diff(second_row) + first_col.abs_diff(second_col)
}

#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum QuantizedModelError {
    #[error("hidden size must be between 1 and 256")]
    InvalidHiddenSize,
    #[error("input weight count does not match 16 * 63 * hidden_size")]
    InvalidInputWeights,
    #[error("hidden bias count does not match hidden_size")]
    InvalidHiddenBias,
    #[error("output weight count does not match hidden_size")]
    InvalidOutputWeights,
    #[error("quantization scale must be positive")]
    InvalidScale,
}

#[derive(Clone, Debug)]
struct QuantizedModel {
    hidden_size: usize,
    input_weights: Box<[i16]>,
    hidden_bias: Box<[i32]>,
    output_weights: Box<[i16]>,
    output_bias: i32,
    scale: i32,
}

/// Pluggable integer piece-square network with a deterministic classical
/// fallback. No untrained network is ever used implicitly.
#[derive(Clone, Debug, Default)]
pub struct QuantizedEvaluator {
    model: Option<QuantizedModel>,
    fallback: ClassicalEvaluator,
}

impl QuantizedEvaluator {
    pub fn fallback() -> Self {
        Self::default()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_weights(
        hidden_size: usize,
        input_weights: Vec<i16>,
        hidden_bias: Vec<i32>,
        output_weights: Vec<i16>,
        output_bias: i32,
        scale: i32,
    ) -> Result<Self, QuantizedModelError> {
        if !(1..=256).contains(&hidden_size) {
            return Err(QuantizedModelError::InvalidHiddenSize);
        }
        if input_weights.len() != 16 * BOARD_SIZE * hidden_size {
            return Err(QuantizedModelError::InvalidInputWeights);
        }
        if hidden_bias.len() != hidden_size {
            return Err(QuantizedModelError::InvalidHiddenBias);
        }
        if output_weights.len() != hidden_size {
            return Err(QuantizedModelError::InvalidOutputWeights);
        }
        if scale <= 0 {
            return Err(QuantizedModelError::InvalidScale);
        }
        Ok(Self {
            model: Some(QuantizedModel {
                hidden_size,
                input_weights: input_weights.into_boxed_slice(),
                hidden_bias: hidden_bias.into_boxed_slice(),
                output_weights: output_weights.into_boxed_slice(),
                output_bias,
                scale,
            }),
            fallback: ClassicalEvaluator,
        })
    }

    pub fn is_fallback(&self) -> bool {
        self.model.is_none()
    }

    fn evaluate_model(model: &QuantizedModel, position: &Position) -> i32 {
        let mut hidden = model.hidden_bias.to_vec();
        for square in 0..BOARD_SIZE {
            let code = position.board_codes()[square];
            if code == 0 {
                continue;
            }
            let feature = (code as usize - 1) * BOARD_SIZE + square;
            let offset = feature * model.hidden_size;
            for (unit, value) in hidden.iter_mut().enumerate() {
                *value += i32::from(model.input_weights[offset + unit]);
            }
        }
        let mut output = model.output_bias;
        for (unit, value) in hidden.into_iter().enumerate() {
            let activated = value.clamp(0, 127);
            output = output.saturating_add(activated * i32::from(model.output_weights[unit]));
        }
        let absolute = (output / model.scale).clamp(-45_000, 45_000);
        if position.side_to_move() == Side::Blue {
            absolute
        } else {
            -absolute
        }
    }
}

impl Evaluator for QuantizedEvaluator {
    fn evaluate(&self, position: &Position) -> i32 {
        self.model.as_ref().map_or_else(
            || self.fallback.evaluate(position),
            |model| Self::evaluate_model(model, position),
        )
    }

    fn id(&self) -> &'static str {
        if self.model.is_some() {
            "quantized-piece-square-v1"
        } else {
            "classical-v3-fallback"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jungle_core::Session;

    #[test]
    fn classical_evaluation_is_color_symmetric_at_start() {
        let position = Position::initial();
        let score = ClassicalEvaluator.evaluate(&position);
        assert!(
            score.abs() < 200,
            "initial score should be near balanced: {score}"
        );
    }

    #[test]
    fn classical_material_scale_preserves_the_promoted_reference_weights() {
        assert_eq!(PIECE_VALUES, [0, 145, 235, 320, 405, 510, 755, 825, 900]);
    }

    #[test]
    fn classical_fixture_matches_the_promoted_reference_evaluator() {
        let mut session = Session::new();
        for (origin, destination) in [
            (48, 41),
            (0, 1),
            (41, 34),
            (1, 2),
            (34, 27),
            (20, 19),
            (27, 20),
            (18, 17),
            (20, 13),
            (6, 13),
        ] {
            session.apply_coordinates(origin, destination).unwrap();
        }
        assert_eq!(ClassicalEvaluator.evaluate(session.position()), -683);
    }

    #[test]
    fn quantized_model_validates_shape_and_fallback_is_exact() {
        let fallback = QuantizedEvaluator::fallback();
        let position = Position::initial();
        assert!(fallback.is_fallback());
        assert_eq!(
            fallback.evaluate(&position),
            ClassicalEvaluator.evaluate(&position)
        );
        assert_eq!(
            QuantizedEvaluator::from_weights(4, vec![], vec![0; 4], vec![0; 4], 0, 1)
                .expect_err("shape"),
            QuantizedModelError::InvalidInputWeights
        );
    }

    #[test]
    fn zero_quantized_model_is_deterministic() {
        let hidden = 4;
        let model = QuantizedEvaluator::from_weights(
            hidden,
            vec![0; 16 * BOARD_SIZE * hidden],
            vec![1; hidden],
            vec![1; hidden],
            0,
            1,
        )
        .expect("model");
        let position = Position::initial();
        assert_eq!(model.evaluate(&position), -4);
        assert_eq!(model.evaluate(&position), model.evaluate(&position));
    }
}
