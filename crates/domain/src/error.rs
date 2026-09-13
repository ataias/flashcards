use crate::{CardId, DeckId, ScheduleError, UserId};

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
    InvalidUsername,
    UsernameTaken,
    EmptyPassword,
    UserNotFound {
        user_id: UserId,
    },
    SessionNotFound,
    InvalidCredentials,
    UserDisabled,
    BootstrapNotAllowed,
    NotAdmin,
    CannotModifySelf,
    CannotRemoveLastAdmin,
    /// Argon2id hash or verify failed (corrupt stored hash, RNG, or params).
    PasswordHash,
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
            Self::InvalidUsername => write!(
                f,
                "username must be 3–32 characters and use only A–Z, a–z, 0–9, _, ., or -"
            ),
            Self::UsernameTaken => write!(f, "username is already taken"),
            Self::EmptyPassword => write!(f, "password cannot be empty"),
            Self::UserNotFound { user_id } => write!(f, "user {user_id} not found"),
            Self::SessionNotFound => write!(f, "session not found"),
            Self::InvalidCredentials => write!(f, "invalid username or password"),
            Self::UserDisabled => write!(f, "user is disabled"),
            Self::BootstrapNotAllowed => write!(f, "bootstrap is only allowed when no users exist"),
            Self::NotAdmin => write!(f, "admin privileges required"),
            Self::CannotModifySelf => write!(f, "cannot modify your own account"),
            Self::CannotRemoveLastAdmin => write!(f, "cannot remove the last admin"),
            Self::PasswordHash => write!(f, "failed to hash or verify password"),
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
            | Self::CardNotFound { .. }
            | Self::InvalidUsername
            | Self::UsernameTaken
            | Self::EmptyPassword
            | Self::UserNotFound { .. }
            | Self::SessionNotFound
            | Self::InvalidCredentials
            | Self::UserDisabled
            | Self::BootstrapNotAllowed
            | Self::NotAdmin
            | Self::CannotModifySelf
            | Self::CannotRemoveLastAdmin
            | Self::PasswordHash => None,
        }
    }
}

impl From<ScheduleError> for Error {
    fn from(err: ScheduleError) -> Self {
        Self::Schedule(err)
    }
}
