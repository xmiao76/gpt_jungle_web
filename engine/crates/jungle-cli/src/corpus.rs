use std::collections::HashSet;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use jungle_core::{GameStateDto, Session};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CORPUS_SCHEMA: &str = "jungle-opening-corpus";
pub const CORPUS_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Opening {
    pub id: String,
    pub state: GameStateDto,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpeningCorpus {
    pub schema: String,
    pub version: u32,
    pub name: String,
    pub openings: Vec<Opening>,
}

#[derive(Debug, Error)]
pub enum CorpusError {
    #[error("could not read corpus: {0}")]
    Io(#[from] io::Error),
    #[error("corpus is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported corpus schema {actual:?}; expected {CORPUS_SCHEMA:?}")]
    Schema { actual: String },
    #[error("unsupported corpus version {actual}; expected {CORPUS_VERSION}")]
    Version { actual: u32 },
    #[error("corpus must contain at least one opening")]
    Empty,
    #[error("opening id must not be empty")]
    EmptyId,
    #[error("opening id {0:?} is duplicated")]
    DuplicateId(String),
    #[error("opening {id:?} is invalid: {message}")]
    InvalidPosition { id: String, message: String },
    #[error("opening {0:?} is already terminal")]
    Terminal(String),
}

impl OpeningCorpus {
    #[must_use]
    pub fn smoke() -> Self {
        Self {
            schema: CORPUS_SCHEMA.to_owned(),
            version: CORPUS_VERSION,
            name: "smoke-v1".to_owned(),
            openings: vec![Opening {
                id: "initial".to_owned(),
                state: Session::new().game_state_dto(),
                tags: vec!["smoke".to_owned(), "initial".to_owned()],
            }],
        }
    }

    pub fn from_reader(mut reader: impl Read) -> Result<Self, CorpusError> {
        let mut json = String::new();
        reader.read_to_string(&mut json)?;
        let corpus: Self = serde_json::from_str(&json)?;
        corpus.validate_structure()?;
        Ok(corpus)
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, CorpusError> {
        Self::from_reader(File::open(path)?)
    }

    pub fn validate_structure(&self) -> Result<(), CorpusError> {
        if self.schema != CORPUS_SCHEMA {
            return Err(CorpusError::Schema {
                actual: self.schema.clone(),
            });
        }
        if self.version != CORPUS_VERSION {
            return Err(CorpusError::Version {
                actual: self.version,
            });
        }
        if self.openings.is_empty() {
            return Err(CorpusError::Empty);
        }
        let mut ids = HashSet::with_capacity(self.openings.len());
        for opening in &self.openings {
            if opening.id.trim().is_empty() {
                return Err(CorpusError::EmptyId);
            }
            if !ids.insert(opening.id.as_str()) {
                return Err(CorpusError::DuplicateId(opening.id.clone()));
            }
        }
        Ok(())
    }

    pub fn validate_positions<E>(
        &self,
        mut validate: impl FnMut(&GameStateDto) -> Result<bool, E>,
    ) -> Result<(), CorpusError>
    where
        E: std::fmt::Display,
    {
        for opening in &self.openings {
            let terminal =
                validate(&opening.state).map_err(|error| CorpusError::InvalidPosition {
                    id: opening.id.clone(),
                    message: error.to_string(),
                })?;
            if terminal {
                return Err(CorpusError::Terminal(opening.id.clone()));
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn selected(&self, maximum: Option<usize>) -> &[Opening] {
        let count = maximum
            .unwrap_or(self.openings.len())
            .min(self.openings.len());
        &self.openings[..count]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_smoke_corpus_is_structurally_and_semantically_valid() {
        let corpus = OpeningCorpus::smoke();
        corpus.validate_structure().expect("structure");
        corpus
            .validate_positions(|state| {
                Session::from_game_state_dto(state.clone())
                    .map(|session| session.outcome().is_over())
            })
            .expect("positions");
        assert_eq!(corpus.openings.len(), 1);
    }

    #[test]
    fn duplicate_opening_ids_are_rejected() {
        let mut corpus = OpeningCorpus::smoke();
        corpus.openings.push(corpus.openings[0].clone());
        assert!(matches!(
            corpus.validate_structure(),
            Err(CorpusError::DuplicateId(_))
        ));
    }
}
