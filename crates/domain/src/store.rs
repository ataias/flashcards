use std::future::Future;

use chrono::{DateTime, Utc};

use crate::{Card, CardId, CardText, Deck, DeckId, Error, ReviewLogEntry, Session, User, UserId};

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
    fn get_deck(
        &self,
        user_id: UserId,
        deck_id: DeckId,
    ) -> impl Future<Output = Result<Option<Deck>, Error>> + Send;

    fn create_deck(
        &self,
        user_id: UserId,
        name: &str,
    ) -> impl Future<Output = Result<Deck, Error>> + Send;

    fn rename_deck(
        &self,
        user_id: UserId,
        deck_id: DeckId,
        name: &str,
    ) -> impl Future<Output = Result<Deck, Error>> + Send;

    fn delete_deck(
        &self,
        user_id: UserId,
        deck_id: DeckId,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    fn get_card(
        &self,
        user_id: UserId,
        card_id: CardId,
    ) -> impl Future<Output = Result<Option<Card>, Error>> + Send;

    fn list_card_text_in_deck(
        &self,
        user_id: UserId,
        deck_id: DeckId,
    ) -> impl Future<Output = Result<Vec<CardText>, Error>> + Send;

    fn create_card(
        &self,
        user_id: UserId,
        deck_id: DeckId,
        front: &str,
        back: &str,
    ) -> impl Future<Output = Result<Card, Error>> + Send;

    fn update_card(
        &self,
        user_id: UserId,
        card_id: CardId,
        front: &str,
        back: &str,
    ) -> impl Future<Output = Result<Card, Error>> + Send;

    fn delete_card(
        &self,
        user_id: UserId,
        card_id: CardId,
    ) -> impl Future<Output = Result<DeckId, Error>> + Send;

    fn load_home_inputs(
        &self,
        user_id: UserId,
    ) -> impl Future<Output = Result<HomeInputs, Error>> + Send;

    fn load_study_inputs(
        &self,
        user_id: UserId,
        deck_id: DeckId,
    ) -> impl Future<Output = Result<Option<StudyInputs>, Error>> + Send;

    fn commit_review(
        &self,
        user_id: UserId,
        card: &Card,
        entry: &ReviewLogEntry,
    ) -> impl Future<Output = Result<(Card, ReviewLogEntry), Error>> + Send;

    fn get_user(&self, user_id: UserId)
    -> impl Future<Output = Result<Option<User>, Error>> + Send;

    /// Case-insensitive username lookup; stored username case is unchanged.
    fn get_user_by_username(
        &self,
        username: &str,
    ) -> impl Future<Output = Result<Option<User>, Error>> + Send;

    fn list_users(&self) -> impl Future<Output = Result<Vec<User>, Error>> + Send;

    fn create_user(
        &self,
        username: &str,
        password_hash: &str,
        admin: bool,
    ) -> impl Future<Output = Result<User, Error>> + Send;

    fn set_password_hash(
        &self,
        user_id: UserId,
        password_hash: &str,
    ) -> impl Future<Output = Result<User, Error>> + Send;

    fn set_disabled(
        &self,
        user_id: UserId,
        disabled: bool,
    ) -> impl Future<Output = Result<User, Error>> + Send;

    fn delete_user(&self, user_id: UserId) -> impl Future<Output = Result<(), Error>> + Send;

    fn create_session(
        &self,
        user_id: UserId,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<Session, Error>> + Send;

    fn get_session(
        &self,
        session_id: &str,
    ) -> impl Future<Output = Result<Option<Session>, Error>> + Send;

    fn delete_session(&self, session_id: &str) -> impl Future<Output = Result<(), Error>> + Send;

    fn delete_sessions_for_user(
        &self,
        user_id: UserId,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
