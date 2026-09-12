use askama::Template;
use axum::extract::{Form, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use chrono::{Local, Utc};
use db::{Rating, SqlitePool};
use serde::Deserialize;

use crate::error::AppError;
use crate::wants_fragment;

#[derive(Template)]
#[template(path = "study.html")]
struct StudyPageTemplate {
    deck_id: i64,
    deck_name: String,
    card: Option<StudyCard>,
    revealed: bool,
    error: Option<String>,
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
}

#[derive(Deserialize)]
pub struct RateForm {
    rating: i64,
}

pub async fn study_page(
    State(pool): State<SqlitePool>,
    Path(deck_id): Path<i64>,
) -> Result<Response, AppError> {
    let Some(deck) = db::get_deck(&pool, deck_id).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let card = next_card(&pool, deck_id).await?;
    render_full(&deck, card, false, None)
}

pub async fn reveal(
    State(pool): State<SqlitePool>,
    Path(card_id): Path<i64>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let Some(card) = db::get_card(&pool, card_id).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    render_review(
        &pool,
        card.deck_id,
        Some(study_card(&card)),
        true,
        None,
        &headers,
    )
    .await
}

pub async fn rate(
    State(pool): State<SqlitePool>,
    Path(card_id): Path<i64>,
    headers: HeaderMap,
    Form(form): Form<RateForm>,
) -> Result<Response, AppError> {
    let Some(card) = db::get_card(&pool, card_id).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let Some(rating) = Rating::from_grade(form.rating) else {
        return render_review(
            &pool,
            card.deck_id,
            Some(study_card(&card)),
            true,
            Some("Choose Again, Hard, Good, or Easy."),
            &headers,
        )
        .await;
    };
    match db::rate_card(&pool, &card, rating, Utc::now()).await {
        Ok(_) => after_rate(&pool, card.deck_id, &headers).await,
        Err(db::Error::CardNotFound { .. }) => Ok(StatusCode::NOT_FOUND.into_response()),
        Err(err) => Err(err.into()),
    }
}

async fn after_rate(
    pool: &SqlitePool,
    deck_id: i64,
    headers: &HeaderMap,
) -> Result<Response, AppError> {
    if wants_fragment(headers) {
        let card = next_card(pool, deck_id).await?;
        render_review(pool, deck_id, card, false, None, headers).await
    } else {
        Ok(Redirect::to(&format!("/decks/{deck_id}/study")).into_response())
    }
}

async fn render_review(
    pool: &SqlitePool,
    deck_id: i64,
    card: Option<StudyCard>,
    revealed: bool,
    error: Option<&str>,
    headers: &HeaderMap,
) -> Result<Response, AppError> {
    let Some(deck) = db::get_deck(pool, deck_id).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
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
    deck: &db::Deck,
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
        }
        .render()?,
    )
    .into_response())
}

async fn next_card(pool: &SqlitePool, deck_id: i64) -> Result<Option<StudyCard>, AppError> {
    Ok(db::study_queue(pool, deck_id, Local::now())
        .await?
        .into_iter()
        .next()
        .map(|card| study_card(&card)))
}

fn study_card(card: &db::Card) -> StudyCard {
    StudyCard {
        id: card.id,
        front: card.front.clone(),
        back: card.back.clone(),
    }
}
