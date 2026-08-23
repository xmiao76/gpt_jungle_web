use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{TablebaseError, hex_decode, hex_encode};

pub use jungle_core::PieceKind;

/// Exact material counts, independent of square placement and side-to-move.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct MaterialSignature {
    pub blue: [u8; 8],
    pub red: [u8; 8],
}

impl MaterialSignature {
    pub const fn new(blue: [u8; 8], red: [u8; 8]) -> Self {
        Self { blue, red }
    }

    pub fn one_vs_one(blue: PieceKind, red: PieceKind) -> Self {
        let mut blue_counts = [0; 8];
        let mut red_counts = [0; 8];
        blue_counts[(blue.rank() - 1) as usize] = 1;
        red_counts[(red.rank() - 1) as usize] = 1;
        Self::new(blue_counts, red_counts)
    }

    pub fn validate(self) -> Result<(), TablebaseError> {
        let blue_total: u16 = self.blue.iter().map(|&count| u16::from(count)).sum();
        let red_total: u16 = self.red.iter().map(|&count| u16::from(count)).sum();
        if blue_total + red_total == 0 {
            return Err(TablebaseError::InvalidFormat(
                "material signature cannot be empty".into(),
            ));
        }
        if blue_total > 8 || red_total > 8 {
            return Err(TablebaseError::InvalidFormat(format!(
                "material signature exceeds Jungle's eight pieces per side ({blue_total} blue, {red_total} red)"
            )));
        }
        Ok(())
    }

    pub fn total_pieces(self) -> u16 {
        self.blue
            .iter()
            .chain(self.red.iter())
            .map(|&count| u16::from(count))
            .sum()
    }

    pub fn from_position(position: &jungle_core::Position) -> Self {
        let mut blue = [0; 8];
        let mut red = [0; 8];
        for &code in position.board_codes() {
            match code {
                1..=8 => blue[usize::from(code - 1)] += 1,
                9..=16 => red[usize::from(code - 9)] += 1,
                _ => {}
            }
        }
        Self::new(blue, red)
    }

    pub(crate) fn as_bytes(self) -> [u8; 16] {
        let mut bytes = [0; 16];
        bytes[..8].copy_from_slice(&self.blue);
        bytes[8..].copy_from_slice(&self.red);
        bytes
    }

    pub(crate) fn from_bytes(bytes: [u8; 16]) -> Result<Self, TablebaseError> {
        let mut blue = [0; 8];
        let mut red = [0; 8];
        blue.copy_from_slice(&bytes[..8]);
        red.copy_from_slice(&bytes[8..]);
        let signature = Self::new(blue, red);
        signature.validate()?;
        Ok(signature)
    }
}

impl fmt::Display for MaterialSignature {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn write_side(
            formatter: &mut fmt::Formatter<'_>,
            prefix: char,
            counts: &[u8; 8],
        ) -> fmt::Result {
            write!(formatter, "{prefix}")?;
            for (kind, count) in PieceKind::ALL.iter().zip(counts) {
                if *count != 0 {
                    write!(formatter, "{}{}", kind.rank(), count)?;
                }
            }
            Ok(())
        }

        write_side(formatter, 'b', &self.blue)?;
        formatter.write_str("-")?;
        write_side(formatter, 'r', &self.red)
    }
}

/// A collision-free, canonical encoding within one material signature.
///
/// The format intentionally treats this as an opaque 128-bit ordinal so the
/// canonical game-state codec can evolve independently of tablebase storage.
#[derive(Clone, Copy, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PositionKey(pub u128);

impl PositionKey {
    pub const fn new(value: u128) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u128 {
        self.0
    }

    pub fn to_hex(self) -> String {
        hex_encode(&self.0.to_be_bytes())
    }

    pub fn from_hex(value: &str) -> Result<Self, TablebaseError> {
        Ok(Self(u128::from_be_bytes(hex_decode(value)?)))
    }

    /// Encode a canonical position collision-free within its material signature.
    /// Pieces are ordered by public compact code, each square occupies six bits,
    /// and side-to-move is the final low bit.
    pub fn from_position(position: &jungle_core::Position) -> Self {
        let mut value = 0_u128;
        for wanted_code in 1..=16 {
            if let Some((square, _)) = position
                .board_codes()
                .iter()
                .enumerate()
                .find(|(_, code)| **code == wanted_code)
            {
                value = value * 64 + square as u128;
            }
        }
        value = value * 2 + position.side_to_move().index() as u128;
        Self(value)
    }
}

impl fmt::Debug for PositionKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("PositionKey")
            .field(&self.to_hex())
            .finish()
    }
}

impl fmt::Display for PositionKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

impl Serialize for PositionKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for PositionKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_hex(&value).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_key_json_is_fixed_width_and_round_trips() {
        let key = PositionKey::new(0x1234);
        let json = serde_json::to_string(&key).unwrap();
        assert_eq!(json, "\"00000000000000000000000000001234\"");
        assert_eq!(serde_json::from_str::<PositionKey>(&json).unwrap(), key);
    }

    #[test]
    fn material_ids_are_deterministic() {
        let signature = MaterialSignature::one_vs_one(PieceKind::Rat, PieceKind::Elephant);
        assert_eq!(signature.to_string(), "b11-r81");
        assert_eq!(signature.total_pieces(), 2);
        signature.validate().unwrap();
    }

    #[test]
    fn position_codec_is_collision_free_for_fixed_material() {
        use jungle_core::{Piece, Position, Side};

        let a = Position::from_piece_placements_checked(
            &[
                (0, Piece::new(Side::Blue, PieceKind::Rat)),
                (62, Piece::new(Side::Red, PieceKind::Elephant)),
            ],
            Side::Blue,
        )
        .unwrap();
        let b = Position::from_piece_placements_checked(
            &[
                (0, Piece::new(Side::Blue, PieceKind::Rat)),
                (62, Piece::new(Side::Red, PieceKind::Elephant)),
            ],
            Side::Red,
        )
        .unwrap();
        assert_eq!(
            MaterialSignature::from_position(&a),
            MaterialSignature::from_position(&b)
        );
        assert_ne!(
            PositionKey::from_position(&a),
            PositionKey::from_position(&b)
        );
    }
}
