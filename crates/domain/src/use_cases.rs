use chrono::{DateTime, TimeZone, Utc};

use crate::home::summarize_home;
use crate::store::Store;
use crate::{
    Card, CardId, CardText, Deck, DeckId, DeckSummary, Error, Rating, ReviewLogEntry,
    select_study_queue,
};

pub async fn list_home<S, Tz>(store: &S, now: DateTime<Tz>) -> Result<Vec<DeckSummary>, Error>
where
    S: Store,
    Tz: TimeZone,
{
    let inputs = store.load_home_inputs().await?;
    Ok(summarize_home(&inputs, now))
}

pub async fn get_deck<S: Store>(store: &S, deck_id: DeckId) -> Result<Option<Deck>, Error> {
    store.get_deck(deck_id).await
}

pub async fn create_deck<S: Store>(store: &S, name: &str) -> Result<Deck, Error> {
    store.create_deck(&normalize_deck_name(name)?).await
}

pub async fn rename_deck<S: Store>(store: &S, deck_id: DeckId, name: &str) -> Result<Deck, Error> {
    store
        .rename_deck(deck_id, &normalize_deck_name(name)?)
        .await
}

pub async fn delete_deck<S: Store>(store: &S, deck_id: DeckId) -> Result<(), Error> {
    store.delete_deck(deck_id).await
}

pub async fn get_card<S: Store>(store: &S, card_id: CardId) -> Result<Option<Card>, Error> {
    store.get_card(card_id).await
}

pub async fn list_deck_cards<S: Store>(
    store: &S,
    deck_id: DeckId,
) -> Result<(Deck, Vec<CardText>), Error> {
    let deck = store
        .get_deck(deck_id)
        .await?
        .ok_or(Error::DeckNotFound { deck_id })?;
    let cards = store.list_card_text_in_deck(deck_id).await?;
    Ok((deck, cards))
}

pub async fn create_card<S: Store>(
    store: &S,
    deck_id: DeckId,
    front: &str,
    back: &str,
) -> Result<Card, Error> {
    let front = normalize_card_side(front, true)?;
    let back = normalize_card_side(back, false)?;
    store.create_card(deck_id, &front, &back).await
}

pub async fn update_card<S: Store>(
    store: &S,
    card_id: CardId,
    front: &str,
    back: &str,
) -> Result<Card, Error> {
    let front = normalize_card_side(front, true)?;
    let back = normalize_card_side(back, false)?;
    store.update_card(card_id, &front, &back).await
}

pub async fn delete_card<S: Store>(store: &S, card_id: CardId) -> Result<DeckId, Error> {
    store.delete_card(card_id).await
}

pub async fn next_study_card<S, Tz>(
    store: &S,
    deck_id: DeckId,
    now: DateTime<Tz>,
) -> Result<Option<Card>, Error>
where
    S: Store,
    Tz: TimeZone,
{
    let Some(inputs) = store.load_study_inputs(deck_id).await? else {
        return Err(Error::DeckNotFound { deck_id });
    };
    let tz = now.timezone();
    let first_local: Vec<_> = inputs
        .first_reviewed_at
        .iter()
        .map(|instant| instant.with_timezone(&tz))
        .collect();
    Ok(select_study_queue(&inputs.cards, &first_local, now)
        .into_iter()
        .next())
}

pub async fn rate<S: Store>(
    store: &S,
    card_id: CardId,
    rating: Rating,
    now: DateTime<Utc>,
) -> Result<(Card, ReviewLogEntry), Error> {
    let card = store
        .get_card(card_id)
        .await?
        .ok_or(Error::CardNotFound { card_id })?;
    let (updated, entry) = card.apply_rating(rating, now)?;
    store.commit_review(&updated, &entry).await
}

fn normalize_deck_name(name: &str) -> Result<String, Error> {
    let name = name.trim();
    if name.is_empty() {
        Err(Error::EmptyDeckName)
    } else {
        Ok(name.to_string())
    }
}

fn normalize_card_side(text: &str, front: bool) -> Result<String, Error> {
    let text = text.trim();
    if text.is_empty() {
        Err(if front {
            Error::EmptyCardFront
        } else {
            Error::EmptyCardBack
        })
    } else {
        Ok(text.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{HomeDeckInput, HomeInputs, StudyInputs};
    use crate::{LEARNING_STEPS, NEW_CARDS_PER_LOCAL_DAY, Phase};
    use chrono::{Duration, FixedOffset, TimeZone};
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Mutex;

    struct Inner {
        next_deck_id: DeckId,
        next_card_id: CardId,
        decks: BTreeMap<DeckId, Deck>,
        cards: BTreeMap<CardId, Card>,
        reviews: Vec<ReviewLogEntry>,
    }

    struct MemStore {
        inner: Mutex<Inner>,
    }

    impl MemStore {
        fn empty() -> Self {
            Self {
                inner: Mutex::new(Inner {
                    next_deck_id: 1,
                    next_card_id: 1,
                    decks: BTreeMap::new(),
                    cards: BTreeMap::new(),
                    reviews: Vec::new(),
                }),
            }
        }

        fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
            self.inner.lock().expect("mem store lock")
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
    }

    impl Store for MemStore {
        async fn get_deck(&self, deck_id: DeckId) -> Result<Option<Deck>, Error> {
            Ok(self.lock().decks.get(&deck_id).cloned())
        }

        async fn create_deck(&self, name: &str) -> Result<Deck, Error> {
            let mut inner = self.lock();
            let id = inner.next_deck_id;
            inner.next_deck_id += 1;
            let deck = Deck {
                id,
                name: name.to_string(),
            };
            inner.decks.insert(id, deck.clone());
            Ok(deck)
        }

        async fn rename_deck(&self, deck_id: DeckId, name: &str) -> Result<Deck, Error> {
            let mut inner = self.lock();
            let deck = inner
                .decks
                .get_mut(&deck_id)
                .ok_or(Error::DeckNotFound { deck_id })?;
            deck.name = name.to_string();
            Ok(deck.clone())
        }

        async fn delete_deck(&self, deck_id: DeckId) -> Result<(), Error> {
            let mut inner = self.lock();
            inner.decks.remove(&deck_id);
            inner.cards.retain(|_, card| card.deck_id != deck_id);
            let remaining: BTreeSet<CardId> = inner.cards.keys().copied().collect();
            inner
                .reviews
                .retain(|entry| remaining.contains(&entry.card_id));
            Ok(())
        }

        async fn get_card(&self, card_id: CardId) -> Result<Option<Card>, Error> {
            Ok(self.lock().cards.get(&card_id).cloned())
        }

        async fn list_card_text_in_deck(&self, deck_id: DeckId) -> Result<Vec<CardText>, Error> {
            let inner = self.lock();
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
            deck_id: DeckId,
            front: &str,
            back: &str,
        ) -> Result<Card, Error> {
            let mut inner = self.lock();
            if !inner.decks.contains_key(&deck_id) {
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
            card_id: CardId,
            front: &str,
            back: &str,
        ) -> Result<Card, Error> {
            let mut inner = self.lock();
            let card = inner
                .cards
                .get_mut(&card_id)
                .ok_or(Error::CardNotFound { card_id })?;
            card.front = front.to_string();
            card.back = back.to_string();
            Ok(card.clone())
        }

        async fn delete_card(&self, card_id: CardId) -> Result<DeckId, Error> {
            let mut inner = self.lock();
            let card = inner
                .cards
                .remove(&card_id)
                .ok_or(Error::CardNotFound { card_id })?;
            inner.reviews.retain(|entry| entry.card_id != card_id);
            Ok(card.deck_id)
        }

        async fn load_home_inputs(&self) -> Result<HomeInputs, Error> {
            let inner = self.lock();
            let decks = inner
                .decks
                .values()
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

        async fn load_study_inputs(&self, deck_id: DeckId) -> Result<Option<StudyInputs>, Error> {
            let inner = self.lock();
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
            card: &Card,
            entry: &ReviewLogEntry,
        ) -> Result<(Card, ReviewLogEntry), Error> {
            let mut inner = self.lock();
            if !inner.cards.contains_key(&card.id) {
                return Err(Error::CardNotFound { card_id: card.id });
            }
            inner.cards.insert(card.id, card.clone());
            inner.reviews.push(entry.clone());
            Ok((card.clone(), entry.clone()))
        }
    }

    fn tz_plus_9() -> FixedOffset {
        FixedOffset::east_opt(9 * 3600).unwrap()
    }

    fn noon_utc() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap()
    }

    #[tokio::test]
    async fn create_deck_trims_and_rejects_empty_name() {
        let store = MemStore::empty();
        let created = create_deck(&store, "  Spanish  ").await.unwrap();
        assert_eq!(created.name, "Spanish");
        assert!(matches!(
            create_deck(&store, "   ").await.unwrap_err(),
            Error::EmptyDeckName
        ));
        assert_eq!(store.lock().decks.len(), 1);
    }

    #[tokio::test]
    async fn deck_and_card_crud_round_trip() {
        let store = MemStore::empty();
        let deck = create_deck(&store, "Default").await.unwrap();
        let created = create_card(&store, deck.id, "  Q  ", "  A  ")
            .await
            .unwrap();
        assert_eq!(created.front, "Q");
        assert_eq!(created.back, "A");
        assert!(created.is_new());

        let (loaded, cards) = list_deck_cards(&store, deck.id).await.unwrap();
        assert_eq!(loaded.name, "Default");
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].front, "Q");
        assert_eq!(cards[0].phase, Phase::New);
        assert_eq!(cards[0].due, None);

        let updated = update_card(&store, created.id, "Q2", "A2").await.unwrap();
        assert_eq!(updated.front, "Q2");
        assert_eq!(updated.back, "A2");

        let renamed = rename_deck(&store, deck.id, "Home").await.unwrap();
        assert_eq!(renamed.name, "Home");

        let deleted_deck_id = delete_card(&store, created.id).await.unwrap();
        assert_eq!(deleted_deck_id, deck.id);
        assert!(get_card(&store, created.id).await.unwrap().is_none());

        delete_deck(&store, deck.id).await.unwrap();
        assert!(get_deck(&store, deck.id).await.unwrap().is_none());
        assert!(matches!(
            list_deck_cards(&store, deck.id).await.unwrap_err(),
            Error::DeckNotFound { deck_id } if deck_id == deck.id
        ));
    }

    #[tokio::test]
    async fn create_card_rejects_empty_sides_and_missing_deck() {
        let store = MemStore::empty();
        let deck = create_deck(&store, "Default").await.unwrap();
        assert!(matches!(
            create_card(&store, deck.id, "   ", "A").await.unwrap_err(),
            Error::EmptyCardFront
        ));
        assert!(matches!(
            create_card(&store, deck.id, "Q", "  ").await.unwrap_err(),
            Error::EmptyCardBack
        ));
        assert!(matches!(
            create_card(&store, 999, "Q", "A").await.unwrap_err(),
            Error::DeckNotFound { deck_id: 999 }
        ));
        assert!(list_deck_cards(&store, deck.id).await.unwrap().1.is_empty());
    }

    #[tokio::test]
    async fn rate_loads_applies_and_commits() {
        let store = MemStore::empty();
        let deck = create_deck(&store, "Default").await.unwrap();
        let card = create_card(&store, deck.id, "Q", "A").await.unwrap();
        let now = noon_utc();

        let (updated, entry) = rate(&store, card.id, Rating::Good, now).await.unwrap();
        let expected = card.apply_rating(Rating::Good, now).unwrap();
        assert_eq!(updated, expected.0);
        assert_eq!(entry, expected.1);

        let stored = get_card(&store, card.id).await.unwrap().unwrap();
        assert_eq!(stored, updated);
        assert_eq!(store.lock().reviews, vec![entry]);
    }

    #[tokio::test]
    async fn rate_missing_card_is_not_found() {
        let store = MemStore::empty();
        assert!(matches!(
            rate(&store, 9, Rating::Good, noon_utc()).await.unwrap_err(),
            Error::CardNotFound { card_id: 9 }
        ));
    }

    #[tokio::test]
    async fn next_study_card_respects_new_cap_and_missing_deck() {
        let store = MemStore::empty();
        let deck = create_deck(&store, "Default").await.unwrap();
        let tz = tz_plus_9();
        let now = tz.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();

        for i in 0..21 {
            create_card(&store, deck.id, &format!("Q{i}"), &format!("A{i}"))
                .await
                .unwrap();
        }
        let first = next_study_card(&store, deck.id, now)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first.front, "Q0");

        let rated_at = now.with_timezone(&Utc);
        for card_id in 1..=20 {
            rate(&store, card_id, Rating::Good, rated_at).await.unwrap();
        }

        let remaining = next_study_card(&store, deck.id, now).await.unwrap();
        assert!(remaining.is_none());

        assert!(matches!(
            next_study_card(&store, 999, now).await.unwrap_err(),
            Error::DeckNotFound { deck_id: 999 }
        ));
    }

    #[tokio::test]
    async fn list_home_applies_new_cap_and_counts_due() {
        let store = MemStore::empty();
        let default = create_deck(&store, "Default").await.unwrap();
        let other = create_deck(&store, "Later").await.unwrap();
        for _ in 0..25 {
            create_card(&store, default.id, "Q", "A").await.unwrap();
        }
        let later = create_card(&store, other.id, "Later Q", "Later A")
            .await
            .unwrap();
        let now = noon_utc();
        rate(&store, later.id, Rating::Good, now).await.unwrap();

        let summaries = list_home(&store, now).await.unwrap();
        let default_summary = summaries.iter().find(|s| s.deck.id == default.id).unwrap();
        let later_summary = summaries.iter().find(|s| s.deck.id == other.id).unwrap();
        assert_eq!(default_summary.new_count, NEW_CARDS_PER_LOCAL_DAY);
        assert_eq!(default_summary.due_count, 0);
        assert_eq!(later_summary.new_count, 0);
        assert_eq!(later_summary.due_count, 0);
    }

    #[tokio::test]
    async fn easy_on_new_consumes_one_new_cap_slot() {
        let store = MemStore::empty();
        let deck = create_deck(&store, "Default").await.unwrap();
        for i in 0..3 {
            create_card(&store, deck.id, &format!("Q{i}"), &format!("A{i}"))
                .await
                .unwrap();
        }
        let now = noon_utc();
        let (updated, entry) = rate(&store, 1, Rating::Easy, now).await.unwrap();
        assert_eq!(updated.phase, Phase::Review);
        assert!(!updated.is_new());
        assert_eq!(entry.rating, Rating::Easy);
        assert_eq!(store.lock().reviews.len(), 1);

        let summaries = list_home(&store, now).await.unwrap();
        assert_eq!(summaries[0].new_count, 2);
        assert_eq!(summaries[0].due_count, 0);

        let next = next_study_card(&store, deck.id, now)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(next.id, 2);
        assert!(next.is_new());
    }

    #[tokio::test]
    async fn further_learning_steps_do_not_consume_another_new_slot() {
        let store = MemStore::empty();
        let deck = create_deck(&store, "Default").await.unwrap();
        for i in 0..3 {
            create_card(&store, deck.id, &format!("Q{i}"), &format!("A{i}"))
                .await
                .unwrap();
        }
        let now = noon_utc();
        rate(&store, 1, Rating::Good, now).await.unwrap();
        rate(&store, 1, Rating::Again, now + Duration::minutes(10))
            .await
            .unwrap();

        let summaries = list_home(&store, now).await.unwrap();
        assert_eq!(summaries[0].new_count, 2);
        assert_eq!(store.lock().reviews.len(), 2);
    }

    #[tokio::test]
    async fn learning_card_returns_to_queue_when_step_elapses() {
        let store = MemStore::empty();
        let deck = create_deck(&store, "Default").await.unwrap();
        create_card(&store, deck.id, "Q", "A").await.unwrap();
        let tz = tz_plus_9();
        let now = tz.with_ymd_and_hms(2026, 9, 11, 12, 0, 0).unwrap();
        rate(&store, 1, Rating::Again, now.with_timezone(&Utc))
            .await
            .unwrap();

        assert!(
            next_study_card(&store, deck.id, now)
                .await
                .unwrap()
                .is_none()
        );
        let later = now + LEARNING_STEPS[0];
        let again = next_study_card(&store, deck.id, later)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(again.phase, Phase::Learning);
        assert_eq!(again.learning_step, Some(0));
    }

    #[tokio::test]
    async fn list_home_counts_due_when_interval_has_elapsed() {
        let store = MemStore::empty();
        let deck = create_deck(&store, "Default").await.unwrap();
        let new = create_card(&store, deck.id, "New", "A").await.unwrap();
        let due = create_card(&store, deck.id, "Due", "A").await.unwrap();
        let now = noon_utc();
        rate(&store, due.id, Rating::Good, now - Duration::days(30))
            .await
            .unwrap();

        let summaries = list_home(&store, now).await.unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].new_count, 1);
        assert_eq!(summaries[0].due_count, 1);
        assert_eq!(
            get_card(&store, new.id).await.unwrap().unwrap().front,
            "New"
        );
    }
}
