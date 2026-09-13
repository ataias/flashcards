use std::path::Path;

use domain::{
    Card, CardId, CardText, Deck, DeckId, Error as DomainError, HomeDeckInput, HomeInputs,
    ReviewLogEntry, Session, Store, StudyInputs, User, UserId,
};

use crate::{SqlitePool, commit_review, open};

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

/// User/Session ports are not implemented in SqliteStore yet. Until then
/// Deck/Card methods ignore `user_id` so the workspace still builds.
fn not_implemented(port: &'static str) -> DomainError {
    DomainError::storage(format!("{port} is not implemented in SqliteStore yet"))
}

impl Store for SqliteStore {
    async fn get_deck(
        &self,
        _user_id: UserId,
        deck_id: DeckId,
    ) -> Result<Option<Deck>, DomainError> {
        from_db(crate::get_deck(&self.pool, deck_id).await)
    }

    async fn create_deck(&self, _user_id: UserId, name: &str) -> Result<Deck, DomainError> {
        from_db(crate::create_deck(&self.pool, name).await)
    }

    async fn rename_deck(
        &self,
        _user_id: UserId,
        deck_id: DeckId,
        name: &str,
    ) -> Result<Deck, DomainError> {
        from_db(crate::rename_deck(&self.pool, deck_id, name).await)
    }

    async fn delete_deck(&self, _user_id: UserId, deck_id: DeckId) -> Result<(), DomainError> {
        from_db(crate::delete_deck(&self.pool, deck_id).await)
    }

    async fn get_card(
        &self,
        _user_id: UserId,
        card_id: CardId,
    ) -> Result<Option<Card>, DomainError> {
        from_db(crate::get_card(&self.pool, card_id).await)
    }

    async fn list_card_text_in_deck(
        &self,
        _user_id: UserId,
        deck_id: DeckId,
    ) -> Result<Vec<CardText>, DomainError> {
        from_db(crate::list_card_text_in_deck(&self.pool, deck_id).await)
    }

    async fn create_card(
        &self,
        _user_id: UserId,
        deck_id: DeckId,
        front: &str,
        back: &str,
    ) -> Result<Card, DomainError> {
        from_db(crate::create_card(&self.pool, deck_id, front, back).await)
    }

    async fn update_card(
        &self,
        _user_id: UserId,
        card_id: CardId,
        front: &str,
        back: &str,
    ) -> Result<Card, DomainError> {
        from_db(crate::update_card(&self.pool, card_id, front, back).await)
    }

    async fn delete_card(&self, _user_id: UserId, card_id: CardId) -> Result<DeckId, DomainError> {
        from_db(crate::delete_card(&self.pool, card_id).await)
    }

    async fn load_home_inputs(&self, _user_id: UserId) -> Result<HomeInputs, DomainError> {
        let decks = from_db(crate::list_decks(&self.pool).await)?;
        let mut home_decks = Vec::with_capacity(decks.len());
        for deck in decks {
            let cards = from_db(crate::list_cards_in_deck(&self.pool, deck.id).await)?;
            let first_reviewed_at = from_db(crate::first_review_times(&self.pool, deck.id).await)?;
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
        _user_id: UserId,
        deck_id: DeckId,
    ) -> Result<Option<StudyInputs>, DomainError> {
        let Some(deck) = from_db(crate::get_deck(&self.pool, deck_id).await)? else {
            return Ok(None);
        };
        let cards = from_db(crate::list_cards_in_deck(&self.pool, deck_id).await)?;
        let first_reviewed_at = from_db(crate::first_review_times(&self.pool, deck_id).await)?;
        Ok(Some(StudyInputs {
            deck,
            cards,
            first_reviewed_at,
        }))
    }

    async fn commit_review(
        &self,
        _user_id: UserId,
        card: &Card,
        entry: &ReviewLogEntry,
    ) -> Result<(Card, ReviewLogEntry), DomainError> {
        from_db(commit_review(&self.pool, card, entry).await)
    }

    async fn get_user(&self, _user_id: UserId) -> Result<Option<User>, DomainError> {
        Err(not_implemented("get_user"))
    }

    async fn get_user_by_username(&self, _username: &str) -> Result<Option<User>, DomainError> {
        Err(not_implemented("get_user_by_username"))
    }

    async fn list_users(&self) -> Result<Vec<User>, DomainError> {
        Err(not_implemented("list_users"))
    }

    async fn create_user(
        &self,
        _username: &str,
        _password_hash: &str,
        _admin: bool,
    ) -> Result<User, DomainError> {
        Err(not_implemented("create_user"))
    }

    async fn set_password_hash(
        &self,
        _user_id: UserId,
        _password_hash: &str,
    ) -> Result<User, DomainError> {
        Err(not_implemented("set_password_hash"))
    }

    async fn set_disabled(&self, _user_id: UserId, _disabled: bool) -> Result<User, DomainError> {
        Err(not_implemented("set_disabled"))
    }

    async fn delete_user(&self, _user_id: UserId) -> Result<(), DomainError> {
        Err(not_implemented("delete_user"))
    }

    async fn create_session(
        &self,
        _user_id: UserId,
        _now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Session, DomainError> {
        Err(not_implemented("create_session"))
    }

    async fn get_session(&self, _session_id: &str) -> Result<Option<Session>, DomainError> {
        Err(not_implemented("get_session"))
    }

    async fn delete_session(&self, _session_id: &str) -> Result<(), DomainError> {
        Err(not_implemented("delete_session"))
    }

    async fn delete_sessions_for_user(&self, _user_id: UserId) -> Result<(), DomainError> {
        Err(not_implemented("delete_sessions_for_user"))
    }

    async fn assign_orphan_decks(&self, _user_id: UserId) -> Result<usize, DomainError> {
        Err(not_implemented("assign_orphan_decks"))
    }
}
