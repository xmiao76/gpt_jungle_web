use serde::{Deserialize, Serialize};

use crate::TablebaseError;

const DISTANCE_MASK: u16 = 0x3fff;
const DRAW_CODE: u16 = 0x4000;
const WIN_CODE: u16 = 0x8000;
const LOSS_CODE: u16 = 0xc000;

/// Exact game-theoretic result from the side-to-move's point of view.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Wdl {
    Loss,
    Draw,
    Win,
}

/// An exact result and distance-to-conversion in plies.
///
/// Draws have no distance. Wins and losses always do, including terminal
/// positions whose distance is zero.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct TablebaseHit {
    pub wdl: Wdl,
    pub distance: Option<u16>,
}

impl TablebaseHit {
    pub fn new(wdl: Wdl, distance: Option<u16>) -> Result<Self, TablebaseError> {
        let hit = Self { wdl, distance };
        hit.validate()?;
        Ok(hit)
    }

    pub const fn win(distance: u16) -> Self {
        Self {
            wdl: Wdl::Win,
            distance: Some(distance),
        }
    }

    pub const fn draw() -> Self {
        Self {
            wdl: Wdl::Draw,
            distance: None,
        }
    }

    pub const fn loss(distance: u16) -> Self {
        Self {
            wdl: Wdl::Loss,
            distance: Some(distance),
        }
    }

    pub fn validate(self) -> Result<(), TablebaseError> {
        match (self.wdl, self.distance) {
            (Wdl::Draw, None) => Ok(()),
            (Wdl::Draw, Some(_)) => Err(TablebaseError::InvalidFormat(
                "draw tablebase entries cannot have a distance".into(),
            )),
            (Wdl::Win | Wdl::Loss, Some(distance)) if distance <= DISTANCE_MASK => Ok(()),
            (Wdl::Win | Wdl::Loss, Some(distance)) => Err(TablebaseError::InvalidFormat(format!(
                "tablebase distance {distance} exceeds {DISTANCE_MASK}"
            ))),
            (Wdl::Win | Wdl::Loss, None) => Err(TablebaseError::InvalidFormat(
                "win/loss tablebase entries require a distance".into(),
            )),
        }
    }

    pub(crate) fn encode(self) -> Result<u16, TablebaseError> {
        self.validate()?;
        Ok(match (self.wdl, self.distance) {
            (Wdl::Draw, None) => DRAW_CODE,
            (Wdl::Win, Some(distance)) => WIN_CODE | distance,
            (Wdl::Loss, Some(distance)) => LOSS_CODE | distance,
            _ => unreachable!("validated above"),
        })
    }

    pub(crate) fn decode(value: u16) -> Result<Option<Self>, TablebaseError> {
        let kind = value & 0xc000;
        let distance = value & DISTANCE_MASK;
        match kind {
            0 => {
                if distance == 0 {
                    Ok(None)
                } else {
                    Err(TablebaseError::InvalidFormat(format!(
                        "unknown tablebase entry has nonzero payload {value:#06x}"
                    )))
                }
            }
            DRAW_CODE => {
                if distance == 0 {
                    Ok(Some(Self::draw()))
                } else {
                    Err(TablebaseError::InvalidFormat(format!(
                        "draw tablebase entry has a distance payload {distance}"
                    )))
                }
            }
            WIN_CODE => Ok(Some(Self::win(distance))),
            LOSS_CODE => Ok(Some(Self::loss(distance))),
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_round_trip() {
        for hit in [
            TablebaseHit::loss(0),
            TablebaseHit::loss(34),
            TablebaseHit::draw(),
            TablebaseHit::win(0),
            TablebaseHit::win(16_383),
        ] {
            assert_eq!(
                TablebaseHit::decode(hit.encode().unwrap()).unwrap(),
                Some(hit)
            );
        }
    }

    #[test]
    fn rejects_invalid_entry_encodings() {
        assert!(TablebaseHit::new(Wdl::Draw, Some(1)).is_err());
        assert!(TablebaseHit::new(Wdl::Win, None).is_err());
        assert!(TablebaseHit::decode(1).is_err());
        assert!(TablebaseHit::decode(0x4001).is_err());
    }
}
