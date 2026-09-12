use std::future::Future;

use chrono::{DateTime, Utc};

use crate::{Card, CardId, CardText, Deck, DeckId, Error, ReviewLogEntry};

#[derive(Debug, Clone, PartialEq)]
pub struct HomeDeckInput {
    pub deck: Deck,
    pub cards: Vec<Card>,
    pub first_reviewed_at: Vec<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HomeInputs {
    pub decks: Vec<HomeDeckInput>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StudyInputs {
    pub deck: Deck,
    pub cards: Vec<Card>,
    pub first_reviewed_at: Vec<DateTime<Utc>>,
}

pub trait Store: Send + Sync {
    fn get_deck(&self, deck_id: DeckId)
    -> impl Future<Output = Result<Option<Deck>, Error>> + Send;

    fn create_deck(&self, name: &str) -> impl Future<Output = Result<Deck, Error>> + Send;

    fn rename_deck(
        &self,
        deck_id: DeckId,
        name: &str,
    ) -> impl Future<Output = Result<Deck, Error>> + Send;

    fn delete_deck(&self, deck_id: DeckId) -> impl Future<Output = Result<(), Error>> + Send;

    fn get_card(&self, card_id: CardId)
    -> impl Future<Output = Result<Option<Card>, Error>> + Send;

    fn list_card_text_in_deck(
        &self,
        deck_id: DeckId,
    ) -> impl Future<Output = Result<Vec<CardText>, Error>> + Send;

    fn create_card(
        &self,
        deck_id: DeckId,
        front: &str,
        back: &str,
    ) -> impl Future<Output = Result<Card, Error>> + Send;

    fn update_card(
        &self,
        card_id: CardId,
        front: &str,
        back: &str,
    ) -> impl Future<Output = Result<Card, Error>> + Send;

    fn delete_card(&self, card_id: CardId) -> impl Future<Output = Result<DeckId, Error>> + Send;

    fn load_home_inputs(&self) -> impl Future<Output = Result<HomeInputs, Error>> + Send;

    fn load_study_inputs(
        &self,
        deck_id: DeckId,
    ) -> impl Future<Output = Result<Option<StudyInputs>, Error>> + Send;

    fn commit_review(
        &self,
        card: &Card,
        entry: &ReviewLogEntry,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
