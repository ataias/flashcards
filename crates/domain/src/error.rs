use crate::{CardId, DeckId, ScheduleError};

#[derive(Debug)]
pub enum Error {
    EmptyDeckName,
    EmptyCardFront,
    EmptyCardBack,
    DeckNotFound {
        deck_id: DeckId,
    },
    CardNotFound {
        card_id: CardId,
    },
    Schedule(ScheduleError),
    /// Persistence failure mapped at the Store implementation boundary.
    Storage(Box<dyn std::error::Error + Send + Sync>),
}

impl Error {
    pub fn storage<E>(err: E) -> Self
    where
        E: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        Self::Storage(err.into())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyDeckName => write!(f, "Deck name cannot be empty"),
            Self::EmptyCardFront => write!(f, "Card front cannot be empty"),
            Self::EmptyCardBack => write!(f, "Card back cannot be empty"),
            Self::DeckNotFound { deck_id } => write!(f, "deck {deck_id} not found"),
            Self::CardNotFound { card_id } => write!(f, "card {card_id} not found"),
            Self::Schedule(err) => write!(f, "{err}"),
            Self::Storage(err) => write!(f, "storage error: {err}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Schedule(err) => Some(err),
            Self::Storage(err) => Some(err.as_ref()),
            Self::EmptyDeckName
            | Self::EmptyCardFront
            | Self::EmptyCardBack
            | Self::DeckNotFound { .. }
            | Self::CardNotFound { .. } => None,
        }
    }
}

impl From<ScheduleError> for Error {
    fn from(err: ScheduleError) -> Self {
        Self::Schedule(err)
    }
}
