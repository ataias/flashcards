use askama::Template;
use axum::extract::{Form, Path, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use chrono::{Local, Utc};
use domain::{Deck, Rating, Store};
use serde::Deserialize;

use crate::assets::Head;
use crate::error::AppError;
use crate::session::AuthUser;
use crate::wants_fragment;

#[derive(Template)]
#[template(path = "study.html")]
struct StudyPageTemplate {
    deck_id: i64,
    deck_name: String,
    card: Option<StudyCard>,
    revealed: bool,
    error: Option<String>,
    head: Head,
}

#[derive(Template)]
#[template(path = "review.html")]
struct ReviewTemplate {
    card: Option<StudyCard>,
    revealed: bool,
    error: Option<String>,
}

struct StudyCard {
    id: i64,
    front: String,
    back: String,
    again_interval: String,
    hard_interval: String,
    good_interval: String,
    easy_interval: String,
}

#[derive(Deserialize)]
pub struct RateForm {
    rating: i64,
}

pub async fn study_page<S: Store>(
    State(store): State<S>,
    auth: AuthUser,
    Path(deck_id): Path<i64>,
) -> Result<Response, AppError> {
    let user_id = auth.user.id;
    let deck = domain::get_deck(&store, user_id, deck_id)
        .await?
        .ok_or(domain::Error::DeckNotFound { deck_id })?;
    let card = next_card(&store, user_id, deck_id).await?;
    render_full(&deck, card, false, None)
}

pub async fn reveal<S: Store>(
    State(store): State<S>,
    auth: AuthUser,
    Path(card_id): Path<i64>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let user_id = auth.user.id;
    let card = domain::get_card(&store, user_id, card_id)
        .await?
        .ok_or(domain::Error::CardNotFound { card_id })?;
    render_review(
        &store,
        user_id,
        card.deck_id,
        Some(study_card(&card)?),
        true,
        None,
        &headers,
    )
    .await
}

pub async fn rate<S: Store>(
    State(store): State<S>,
    auth: AuthUser,
    Path(card_id): Path<i64>,
    headers: HeaderMap,
    Form(form): Form<RateForm>,
) -> Result<Response, AppError> {
    let user_id = auth.user.id;
    let Some(rating) = Rating::from_grade(form.rating) else {
        let card = domain::get_card(&store, user_id, card_id)
            .await?
            .ok_or(domain::Error::CardNotFound { card_id })?;
        return render_review(
            &store,
            user_id,
            card.deck_id,
            Some(study_card(&card)?),
            true,
            Some("Choose Again, Hard, Good, or Easy."),
            &headers,
        )
        .await;
    };
    let (card, _) = domain::rate(&store, user_id, card_id, rating, Utc::now()).await?;
    after_rate(&store, user_id, card.deck_id, &headers).await
}

async fn after_rate<S: Store>(
    store: &S,
    user_id: domain::UserId,
    deck_id: i64,
    headers: &HeaderMap,
) -> Result<Response, AppError> {
    if wants_fragment(headers) {
        let card = next_card(store, user_id, deck_id).await?;
        render_review(store, user_id, deck_id, card, false, None, headers).await
    } else {
        Ok(Redirect::to(&format!("/decks/{deck_id}/study")).into_response())
    }
}

async fn render_review<S: Store>(
    store: &S,
    user_id: domain::UserId,
    deck_id: i64,
    card: Option<StudyCard>,
    revealed: bool,
    error: Option<&str>,
    headers: &HeaderMap,
) -> Result<Response, AppError> {
    let deck = domain::get_deck(store, user_id, deck_id)
        .await?
        .ok_or(domain::Error::DeckNotFound { deck_id })?;
    if wants_fragment(headers) {
        Ok(Html(
            ReviewTemplate {
                card,
                revealed,
                error: error.map(str::to_string),
            }
            .render()?,
        )
        .into_response())
    } else {
        render_full(&deck, card, revealed, error)
    }
}

fn render_full(
    deck: &Deck,
    card: Option<StudyCard>,
    revealed: bool,
    error: Option<&str>,
) -> Result<Response, AppError> {
    Ok(Html(
        StudyPageTemplate {
            deck_id: deck.id,
            deck_name: deck.name.clone(),
            card,
            revealed,
            error: error.map(str::to_string),
            head: Head::new(format!("Study — {}", deck.name))?,
        }
        .render()?,
    )
    .into_response())
}

async fn next_card<S: Store>(
    store: &S,
    user_id: domain::UserId,
    deck_id: i64,
) -> Result<Option<StudyCard>, AppError> {
    match domain::next_study_card(store, user_id, deck_id, Local::now()).await? {
        Some(card) => Ok(Some(study_card(&card)?)),
        None => Ok(None),
    }
}

fn study_card(card: &domain::Card) -> Result<StudyCard, AppError> {
    let now = Utc::now();
    Ok(StudyCard {
        id: card.id,
        front: card.front.clone(),
        back: card.back.clone(),
        again_interval: interval_label(card, Rating::Again, now)?,
        hard_interval: interval_label(card, Rating::Hard, now)?,
        good_interval: interval_label(card, Rating::Good, now)?,
        easy_interval: interval_label(card, Rating::Easy, now)?,
    })
}

fn interval_label(
    card: &domain::Card,
    rating: Rating,
    now: chrono::DateTime<Utc>,
) -> Result<String, AppError> {
    card.preview_interval_label(rating, now)
        .map_err(domain::Error::from)
        .map_err(AppError::from)
}
