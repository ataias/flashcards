use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use chrono::{DateTime, Utc};

use crate::store::{HomeDeckInput, HomeInputs, Store, StudyInputs};
use crate::user::{Session, User, usernames_equal};
use crate::{Card, CardId, CardText, Deck, DeckId, Error, ReviewLogEntry, SessionId, UserId};

pub(crate) struct Inner {
    next_user_id: UserId,
    next_deck_id: DeckId,
    next_card_id: CardId,
    next_session_n: u64,
    users: BTreeMap<UserId, User>,
    sessions: BTreeMap<SessionId, Session>,
    pub(crate) decks: BTreeMap<DeckId, Deck>,
    deck_owners: BTreeMap<DeckId, Option<UserId>>,
    pub(crate) cards: BTreeMap<CardId, Card>,
    pub(crate) reviews: Vec<ReviewLogEntry>,
}

pub(crate) struct MemStore {
    inner: Mutex<Inner>,
}

impl MemStore {
    pub(crate) fn empty() -> Self {
        Self {
            inner: Mutex::new(Inner {
                next_user_id: 1,
                next_deck_id: 1,
                next_card_id: 1,
                next_session_n: 1,
                users: BTreeMap::new(),
                sessions: BTreeMap::new(),
                decks: BTreeMap::new(),
                deck_owners: BTreeMap::new(),
                cards: BTreeMap::new(),
                reviews: Vec::new(),
            }),
        }
    }

    pub(crate) fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("mem store lock")
    }

    pub(crate) fn insert_orphan_deck(&self, name: &str) -> Deck {
        let mut inner = self.lock();
        let id = inner.next_deck_id;
        inner.next_deck_id += 1;
        let deck = Deck {
            id,
            name: name.to_string(),
        };
        inner.decks.insert(id, deck.clone());
        inner.deck_owners.insert(id, None);
        deck
    }

    pub(crate) fn insert_user(&self, username: &str, admin: bool) -> User {
        let mut inner = self.lock();
        let id = inner.next_user_id;
        inner.next_user_id += 1;
        let user = User {
            id,
            username: username.to_string(),
            password_hash: "test-hash".into(),
            admin,
            disabled: false,
        };
        inner.users.insert(id, user.clone());
        user
    }

    pub(crate) fn session_count(&self) -> usize {
        self.lock().sessions.len()
    }

    pub(crate) fn sessions_for(&self, user_id: UserId) -> usize {
        self.lock()
            .sessions
            .values()
            .filter(|session| session.user_id == user_id)
            .count()
    }

    fn first_reviews_for_deck(inner: &Inner, deck_id: DeckId) -> Vec<DateTime<Utc>> {
        let mut firsts: BTreeMap<CardId, DateTime<Utc>> = BTreeMap::new();
        for entry in &inner.reviews {
            let Some(card) = inner.cards.get(&entry.card_id) else {
                continue;
            };
            if card.deck_id != deck_id {
                continue;
            }
            firsts.entry(entry.card_id).or_insert(entry.rated_at);
        }
        firsts.into_values().collect()
    }

    fn owns_deck(inner: &Inner, user_id: UserId, deck_id: DeckId) -> bool {
        inner.deck_owners.get(&deck_id) == Some(&Some(user_id))
    }

    fn card_for_user<'a>(inner: &'a Inner, user_id: UserId, card_id: CardId) -> Option<&'a Card> {
        let card = inner.cards.get(&card_id)?;
        if Self::owns_deck(inner, user_id, card.deck_id) {
            Some(card)
        } else {
            None
        }
    }
}

impl Store for MemStore {
    async fn get_deck(&self, user_id: UserId, deck_id: DeckId) -> Result<Option<Deck>, Error> {
        let inner = self.lock();
        if !Self::owns_deck(&inner, user_id, deck_id) {
            return Ok(None);
        }
        Ok(inner.decks.get(&deck_id).cloned())
    }

    async fn create_deck(&self, user_id: UserId, name: &str) -> Result<Deck, Error> {
        let mut inner = self.lock();
        let id = inner.next_deck_id;
        inner.next_deck_id += 1;
        let deck = Deck {
            id,
            name: name.to_string(),
        };
        inner.decks.insert(id, deck.clone());
        inner.deck_owners.insert(id, Some(user_id));
        Ok(deck)
    }

    async fn rename_deck(
        &self,
        user_id: UserId,
        deck_id: DeckId,
        name: &str,
    ) -> Result<Deck, Error> {
        let mut inner = self.lock();
        if !Self::owns_deck(&inner, user_id, deck_id) {
            return Err(Error::DeckNotFound { deck_id });
        }
        let deck = inner
            .decks
            .get_mut(&deck_id)
            .ok_or(Error::DeckNotFound { deck_id })?;
        deck.name = name.to_string();
        Ok(deck.clone())
    }

    async fn delete_deck(&self, user_id: UserId, deck_id: DeckId) -> Result<(), Error> {
        let mut inner = self.lock();
        if !Self::owns_deck(&inner, user_id, deck_id) {
            return Err(Error::DeckNotFound { deck_id });
        }
        inner.decks.remove(&deck_id);
        inner.deck_owners.remove(&deck_id);
        inner.cards.retain(|_, card| card.deck_id != deck_id);
        let remaining: BTreeSet<CardId> = inner.cards.keys().copied().collect();
        inner
            .reviews
            .retain(|entry| remaining.contains(&entry.card_id));
        Ok(())
    }

    async fn get_card(&self, user_id: UserId, card_id: CardId) -> Result<Option<Card>, Error> {
        let inner = self.lock();
        Ok(Self::card_for_user(&inner, user_id, card_id).cloned())
    }

    async fn list_card_text_in_deck(
        &self,
        user_id: UserId,
        deck_id: DeckId,
    ) -> Result<Vec<CardText>, Error> {
        let inner = self.lock();
        if !Self::owns_deck(&inner, user_id, deck_id) {
            return Ok(Vec::new());
        }
        Ok(inner
            .cards
            .values()
            .filter(|card| card.deck_id == deck_id)
            .map(|card| CardText {
                id: card.id,
                front: card.front.clone(),
                back: card.back.clone(),
                phase: card.phase,
                due: card.due,
            })
            .collect())
    }

    async fn create_card(
        &self,
        user_id: UserId,
        deck_id: DeckId,
        front: &str,
        back: &str,
    ) -> Result<Card, Error> {
        let mut inner = self.lock();
        if !Self::owns_deck(&inner, user_id, deck_id) {
            return Err(Error::DeckNotFound { deck_id });
        }
        let id = inner.next_card_id;
        inner.next_card_id += 1;
        let card = Card {
            id,
            deck_id,
            front: front.to_string(),
            back: back.to_string(),
            phase: crate::Phase::New,
            learning_step: None,
            memory: None,
            due: None,
            last_review: None,
        };
        inner.cards.insert(id, card.clone());
        Ok(card)
    }

    async fn update_card(
        &self,
        user_id: UserId,
        card_id: CardId,
        front: &str,
        back: &str,
    ) -> Result<Card, Error> {
        let mut inner = self.lock();
        if Self::card_for_user(&inner, user_id, card_id).is_none() {
            return Err(Error::CardNotFound { card_id });
        }
        let card = inner
            .cards
            .get_mut(&card_id)
            .ok_or(Error::CardNotFound { card_id })?;
        card.front = front.to_string();
        card.back = back.to_string();
        Ok(card.clone())
    }

    async fn delete_card(&self, user_id: UserId, card_id: CardId) -> Result<DeckId, Error> {
        let mut inner = self.lock();
        if Self::card_for_user(&inner, user_id, card_id).is_none() {
            return Err(Error::CardNotFound { card_id });
        }
        let card = inner
            .cards
            .remove(&card_id)
            .ok_or(Error::CardNotFound { card_id })?;
        inner.reviews.retain(|entry| entry.card_id != card_id);
        Ok(card.deck_id)
    }

    async fn load_home_inputs(&self, user_id: UserId) -> Result<HomeInputs, Error> {
        let inner = self.lock();
        let decks = inner
            .decks
            .values()
            .filter(|deck| Self::owns_deck(&inner, user_id, deck.id))
            .map(|deck| {
                let cards = inner
                    .cards
                    .values()
                    .filter(|card| card.deck_id == deck.id)
                    .cloned()
                    .collect();
                HomeDeckInput {
                    deck: deck.clone(),
                    cards,
                    first_reviewed_at: Self::first_reviews_for_deck(&inner, deck.id),
                }
            })
            .collect();
        Ok(HomeInputs { decks })
    }

    async fn load_study_inputs(
        &self,
        user_id: UserId,
        deck_id: DeckId,
    ) -> Result<Option<StudyInputs>, Error> {
        let inner = self.lock();
        if !Self::owns_deck(&inner, user_id, deck_id) {
            return Ok(None);
        }
        let Some(deck) = inner.decks.get(&deck_id).cloned() else {
            return Ok(None);
        };
        let cards = inner
            .cards
            .values()
            .filter(|card| card.deck_id == deck_id)
            .cloned()
            .collect();
        Ok(Some(StudyInputs {
            deck,
            cards,
            first_reviewed_at: Self::first_reviews_for_deck(&inner, deck_id),
        }))
    }

    async fn commit_review(
        &self,
        user_id: UserId,
        card: &Card,
        entry: &ReviewLogEntry,
    ) -> Result<(Card, ReviewLogEntry), Error> {
        let mut inner = self.lock();
        if Self::card_for_user(&inner, user_id, card.id).is_none() {
            return Err(Error::CardNotFound { card_id: card.id });
        }
        inner.cards.insert(card.id, card.clone());
        inner.reviews.push(entry.clone());
        Ok((card.clone(), entry.clone()))
    }

    async fn get_user(&self, user_id: UserId) -> Result<Option<User>, Error> {
        Ok(self.lock().users.get(&user_id).cloned())
    }

    async fn get_user_by_username(&self, username: &str) -> Result<Option<User>, Error> {
        Ok(self
            .lock()
            .users
            .values()
            .find(|user| usernames_equal(&user.username, username))
            .cloned())
    }

    async fn list_users(&self) -> Result<Vec<User>, Error> {
        Ok(self.lock().users.values().cloned().collect())
    }

    async fn create_user(
        &self,
        username: &str,
        password_hash: &str,
        admin: bool,
    ) -> Result<User, Error> {
        let mut inner = self.lock();
        if inner
            .users
            .values()
            .any(|user| usernames_equal(&user.username, username))
        {
            return Err(Error::UsernameTaken);
        }
        let id = inner.next_user_id;
        inner.next_user_id += 1;
        let user = User {
            id,
            username: username.to_string(),
            password_hash: password_hash.to_string(),
            admin,
            disabled: false,
        };
        inner.users.insert(id, user.clone());
        Ok(user)
    }

    async fn set_password_hash(&self, user_id: UserId, password_hash: &str) -> Result<User, Error> {
        let mut inner = self.lock();
        let user = inner
            .users
            .get_mut(&user_id)
            .ok_or(Error::UserNotFound { user_id })?;
        user.password_hash = password_hash.to_string();
        Ok(user.clone())
    }

    async fn set_disabled(&self, user_id: UserId, disabled: bool) -> Result<User, Error> {
        let mut inner = self.lock();
        let user = inner
            .users
            .get_mut(&user_id)
            .ok_or(Error::UserNotFound { user_id })?;
        user.disabled = disabled;
        Ok(user.clone())
    }

    async fn delete_user(&self, user_id: UserId) -> Result<(), Error> {
        let mut inner = self.lock();
        if inner.users.remove(&user_id).is_none() {
            return Err(Error::UserNotFound { user_id });
        }
        inner
            .sessions
            .retain(|_, session| session.user_id != user_id);
        let deck_ids: Vec<DeckId> = inner
            .deck_owners
            .iter()
            .filter_map(|(deck_id, owner)| (*owner == Some(user_id)).then_some(*deck_id))
            .collect();
        for deck_id in deck_ids {
            inner.decks.remove(&deck_id);
            inner.deck_owners.remove(&deck_id);
            inner.cards.retain(|_, card| card.deck_id != deck_id);
        }
        let remaining: BTreeSet<CardId> = inner.cards.keys().copied().collect();
        inner
            .reviews
            .retain(|entry| remaining.contains(&entry.card_id));
        Ok(())
    }

    async fn create_session(&self, user_id: UserId, now: DateTime<Utc>) -> Result<Session, Error> {
        let mut inner = self.lock();
        if !inner.users.contains_key(&user_id) {
            return Err(Error::UserNotFound { user_id });
        }
        let n = inner.next_session_n;
        inner.next_session_n += 1;
        let session = Session {
            id: format!("sess-{n}"),
            user_id,
            created_at: now,
            last_used_at: now,
        };
        inner.sessions.insert(session.id.clone(), session.clone());
        Ok(session)
    }

    async fn get_session(&self, session_id: &str) -> Result<Option<Session>, Error> {
        Ok(self.lock().sessions.get(session_id).cloned())
    }

    async fn delete_session(&self, session_id: &str) -> Result<(), Error> {
        self.lock().sessions.remove(session_id);
        Ok(())
    }

    async fn delete_sessions_for_user(&self, user_id: UserId) -> Result<(), Error> {
        self.lock()
            .sessions
            .retain(|_, session| session.user_id != user_id);
        Ok(())
    }

    async fn assign_orphan_decks(&self, user_id: UserId) -> Result<usize, Error> {
        let mut inner = self.lock();
        if !inner.users.contains_key(&user_id) {
            return Err(Error::UserNotFound { user_id });
        }
        let mut assigned = 0;
        for owner in inner.deck_owners.values_mut() {
            if owner.is_none() {
                *owner = Some(user_id);
                assigned += 1;
            }
        }
        Ok(assigned)
    }
}
