use std::path::Path;

use domain::{
    Card, CardId, CardText, Deck, DeckId, Error as DomainError, HomeDeckInput, HomeInputs,
    ReviewLogEntry, Store, StudyInputs,
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

impl Store for SqliteStore {
    async fn get_deck(&self, deck_id: DeckId) -> Result<Option<Deck>, DomainError> {
        from_db(crate::get_deck(&self.pool, deck_id).await)
    }

    async fn create_deck(&self, name: &str) -> Result<Deck, DomainError> {
        from_db(crate::create_deck(&self.pool, name).await)
    }

    async fn rename_deck(&self, deck_id: DeckId, name: &str) -> Result<Deck, DomainError> {
        from_db(crate::rename_deck(&self.pool, deck_id, name).await)
    }

    async fn delete_deck(&self, deck_id: DeckId) -> Result<(), DomainError> {
        from_db(crate::delete_deck(&self.pool, deck_id).await)
    }

    async fn get_card(&self, card_id: CardId) -> Result<Option<Card>, DomainError> {
        from_db(crate::get_card(&self.pool, card_id).await)
    }

    async fn list_card_text_in_deck(&self, deck_id: DeckId) -> Result<Vec<CardText>, DomainError> {
        from_db(crate::list_card_text_in_deck(&self.pool, deck_id).await)
    }

    async fn create_card(
        &self,
        deck_id: DeckId,
        front: &str,
        back: &str,
    ) -> Result<Card, DomainError> {
        from_db(crate::create_card(&self.pool, deck_id, front, back).await)
    }

    async fn update_card(
        &self,
        card_id: CardId,
        front: &str,
        back: &str,
    ) -> Result<Card, DomainError> {
        from_db(crate::update_card(&self.pool, card_id, front, back).await)
    }

    async fn delete_card(&self, card_id: CardId) -> Result<DeckId, DomainError> {
        from_db(crate::delete_card(&self.pool, card_id).await)
    }

    async fn load_home_inputs(&self) -> Result<HomeInputs, DomainError> {
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

    async fn load_study_inputs(&self, deck_id: DeckId) -> Result<Option<StudyInputs>, DomainError> {
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

    async fn commit_review(&self, card: &Card, entry: &ReviewLogEntry) -> Result<(), DomainError> {
        from_db(commit_review(&self.pool, card, entry).await)
    }
}
