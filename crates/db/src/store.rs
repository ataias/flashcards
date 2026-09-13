use std::path::Path;

use domain::{
    Card, CardId, CardText, Deck, DeckId, Error as DomainError, HomeDeckInput, HomeInputs,
    ReviewLogEntry, Session, Store, StudyInputs, User, UserId,
};

use crate::users;
use crate::{SqlitePool, commit_review, first_review_times, list_cards_in_deck, open};

#[derive(Debug, Clone)]
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn open(path: impl AsRef<Path>) -> Result<Self, crate::Error> {
        Ok(Self {
            pool: open(path).await?,
        })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

fn from_db<T>(result: Result<T, crate::Error>) -> Result<T, DomainError> {
    result.map_err(DomainError::from)
}

impl Store for SqliteStore {
    async fn get_deck(
        &self,
        user_id: UserId,
        deck_id: DeckId,
    ) -> Result<Option<Deck>, DomainError> {
        from_db(users::get_owned_deck(&self.pool, user_id, deck_id).await)
    }

    async fn create_deck(&self, user_id: UserId, name: &str) -> Result<Deck, DomainError> {
        from_db(users::create_owned_deck(&self.pool, user_id, name).await)
    }

    async fn rename_deck(
        &self,
        user_id: UserId,
        deck_id: DeckId,
        name: &str,
    ) -> Result<Deck, DomainError> {
        from_db(users::rename_owned_deck(&self.pool, user_id, deck_id, name).await)
    }

    async fn delete_deck(&self, user_id: UserId, deck_id: DeckId) -> Result<(), DomainError> {
        from_db(users::delete_owned_deck(&self.pool, user_id, deck_id).await)
    }

    async fn get_card(
        &self,
        user_id: UserId,
        card_id: CardId,
    ) -> Result<Option<Card>, DomainError> {
        from_db(users::get_owned_card(&self.pool, user_id, card_id).await)
    }

    async fn list_card_text_in_deck(
        &self,
        user_id: UserId,
        deck_id: DeckId,
    ) -> Result<Vec<CardText>, DomainError> {
        from_db(users::list_owned_card_text(&self.pool, user_id, deck_id).await)
    }

    async fn create_card(
        &self,
        user_id: UserId,
        deck_id: DeckId,
        front: &str,
        back: &str,
    ) -> Result<Card, DomainError> {
        from_db(users::create_owned_card(&self.pool, user_id, deck_id, front, back).await)
    }

    async fn update_card(
        &self,
        user_id: UserId,
        card_id: CardId,
        front: &str,
        back: &str,
    ) -> Result<Card, DomainError> {
        from_db(users::update_owned_card(&self.pool, user_id, card_id, front, back).await)
    }

    async fn delete_card(&self, user_id: UserId, card_id: CardId) -> Result<DeckId, DomainError> {
        from_db(users::delete_owned_card(&self.pool, user_id, card_id).await)
    }

    async fn load_home_inputs(&self, user_id: UserId) -> Result<HomeInputs, DomainError> {
        let decks = from_db(users::list_owned_decks(&self.pool, user_id).await)?;
        let mut home_decks = Vec::with_capacity(decks.len());
        for deck in decks {
            let cards = from_db(list_cards_in_deck(&self.pool, deck.id).await)?;
            let first_reviewed_at = from_db(first_review_times(&self.pool, deck.id).await)?;
            home_decks.push(HomeDeckInput {
                deck,
                cards,
                first_reviewed_at,
            });
        }
        Ok(HomeInputs { decks: home_decks })
    }

    async fn load_study_inputs(
        &self,
        user_id: UserId,
        deck_id: DeckId,
    ) -> Result<Option<StudyInputs>, DomainError> {
        let Some(deck) = from_db(users::get_owned_deck(&self.pool, user_id, deck_id).await)? else {
            return Ok(None);
        };
        let cards = from_db(list_cards_in_deck(&self.pool, deck_id).await)?;
        let first_reviewed_at = from_db(first_review_times(&self.pool, deck_id).await)?;
        Ok(Some(StudyInputs {
            deck,
            cards,
            first_reviewed_at,
        }))
    }

    async fn commit_review(
        &self,
        user_id: UserId,
        card: &Card,
        entry: &ReviewLogEntry,
    ) -> Result<(Card, ReviewLogEntry), DomainError> {
        from_db(users::require_user(&self.pool, user_id).await)?;
        if from_db(users::get_owned_card(&self.pool, user_id, card.id).await)?.is_none() {
            return Err(DomainError::CardNotFound { card_id: card.id });
        }
        from_db(commit_review(&self.pool, card, entry).await)
    }

    async fn get_user(&self, user_id: UserId) -> Result<Option<User>, DomainError> {
        from_db(users::get_user(&self.pool, user_id).await)
    }

    async fn get_user_by_username(&self, username: &str) -> Result<Option<User>, DomainError> {
        from_db(users::get_user_by_username(&self.pool, username).await)
    }

    async fn list_users(&self) -> Result<Vec<User>, DomainError> {
        from_db(users::list_users(&self.pool).await)
    }

    async fn create_user(
        &self,
        username: &str,
        password_hash: &str,
        admin: bool,
    ) -> Result<User, DomainError> {
        from_db(users::create_user(&self.pool, username, password_hash, admin).await)
    }

    async fn set_password_hash(
        &self,
        user_id: UserId,
        password_hash: &str,
    ) -> Result<User, DomainError> {
        from_db(users::set_password_hash(&self.pool, user_id, password_hash).await)
    }

    async fn set_disabled(&self, user_id: UserId, disabled: bool) -> Result<User, DomainError> {
        from_db(users::set_disabled(&self.pool, user_id, disabled).await)
    }

    async fn delete_user(&self, user_id: UserId) -> Result<(), DomainError> {
        from_db(users::delete_user(&self.pool, user_id).await)
    }

    async fn create_session(
        &self,
        user_id: UserId,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Session, DomainError> {
        from_db(users::create_session(&self.pool, user_id, now).await)
    }

    async fn get_session(&self, session_id: &str) -> Result<Option<Session>, DomainError> {
        from_db(users::get_session(&self.pool, session_id).await)
    }

    async fn delete_session(&self, session_id: &str) -> Result<(), DomainError> {
        from_db(users::delete_session(&self.pool, session_id).await)
    }

    async fn delete_sessions_for_user(&self, user_id: UserId) -> Result<(), DomainError> {
        from_db(users::delete_sessions_for_user(&self.pool, user_id).await)
    }

    async fn assign_orphan_decks(&self, user_id: UserId) -> Result<usize, DomainError> {
        from_db(users::assign_orphan_decks(&self.pool, user_id).await)
    }
}
