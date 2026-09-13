use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{HeaderMap, Request, StatusCode, header};
use db::{SqlitePool, SqliteStore};
use tower::ServiceExt;

struct TestDb {
    // Drop pool before TempDir so the SQLite file can be unlinked.
    _dir: tempfile::TempDir,
    pool: SqlitePool,
    store: SqliteStore,
    user_id: domain::UserId,
}

async fn test_db() -> TestDb {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("flashcards.db");
    let pool = db::open(&path).await.unwrap();
    let store = SqliteStore::new(pool.clone());
    let user = domain::bootstrap_admin(&store, "admin", "secret")
        .await
        .unwrap();
    TestDb {
        store,
        pool,
        user_id: user.id,
        _dir: dir,
    }
}

fn app(db: &TestDb) -> Router {
    web::app(db.store.clone(), false)
}

async fn owned_deck(db: &TestDb, name: &str) -> db::Deck {
    domain::create_deck(&db.store, db.user_id, name)
        .await
        .unwrap()
}

async fn request(app: Router, req: Request<Body>) -> (StatusCode, String) {
    let (status, _, body) = request_parts(app, req).await;
    (status, body)
}

async fn request_parts(
    app: Router,
    req: Request<Body>,
) -> (StatusCode, axum::http::HeaderMap, String) {
    let response = app.oneshot(req).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
}

fn cache_control(headers: &axum::http::HeaderMap) -> &str {
    headers
        .get("cache-control")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
}

fn hashed_asset(html: &str, path: &str) -> bool {
    let needle = format!("{path}?h=");
    let Some(index) = html.find(&needle) else {
        return false;
    };
    let hex = html[index + needle.len()..]
        .chars()
        .take(16)
        .collect::<String>();
    hex.len() == 16 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

async fn get(app: Router, path: &str) -> (StatusCode, String) {
    request(
        app,
        Request::builder().uri(path).body(Body::empty()).unwrap(),
    )
    .await
}

async fn rate_http(db: &TestDb, card_id: i64, rating: db::Rating) -> (StatusCode, String) {
    post_form(
        app(db),
        &format!("/cards/{card_id}/rate"),
        &format!("rating={}", rating.as_grade()),
        true,
    )
    .await
}

async fn rate_matches_domain(db: &TestDb, card_id: i64, rating: db::Rating) -> db::Card {
    let before = db::get_card(&db.pool, card_id).await.unwrap().unwrap();
    let (status, _) = rate_http(db, card_id, rating).await;
    assert_eq!(status, StatusCode::OK);
    let after = db::get_card(&db.pool, card_id).await.unwrap().unwrap();
    let rated_at = after.last_review.expect("rating must set last_review");
    let (expected, _) = before.apply_rating(rating, rated_at).unwrap();
    assert_eq!(after, expected);
    after
}

async fn review_ratings(pool: &SqlitePool, card_id: i64) -> Vec<i64> {
    sqlx::query_scalar("SELECT rating FROM review_logs WHERE card_id = ? ORDER BY id")
        .bind(card_id)
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn set_due(pool: &SqlitePool, card_id: i64, due: chrono::DateTime<chrono::Utc>) {
    let due = due.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    sqlx::query("UPDATE cards SET due = ? WHERE id = ?")
        .bind(due)
        .bind(card_id)
        .execute(pool)
        .await
        .unwrap();
}

async fn set_review(pool: &SqlitePool, card_id: i64, due: chrono::DateTime<chrono::Utc>) {
    let last = due - chrono::Duration::days(1);
    sqlx::query(
        "UPDATE cards
         SET phase = 'review', learning_step = NULL,
             stability = 5.0, difficulty = 5.0, due = ?, last_review = ?
         WHERE id = ?",
    )
    .bind(due.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
    .bind(last.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
    .bind(card_id)
    .execute(pool)
    .await
    .unwrap();
}

fn extract_csrf(html: &str) -> String {
    let needle = "name=\"csrf\" value=\"";
    let start = html.find(needle).unwrap_or_else(|| panic!("csrf field")) + needle.len();
    let end = html[start..].find('"').expect("csrf value");
    html[start..start + end].to_string()
}

fn set_cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .find_map(|value| {
            let pair = value.to_str().ok()?.split(';').next()?;
            let (key, val) = pair.split_once('=')?;
            (key.trim() == name).then(|| val.trim().to_string())
        })
}

async fn csrf_cookie(app: Router) -> String {
    let (status, headers, _) = request_parts(
        app,
        Request::builder()
            .uri("/about")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    set_cookie_value(&headers, "csrf").expect("csrf cookie")
}

async fn post_form(app: Router, path: &str, body: &str, htmx: bool) -> (StatusCode, String) {
    let csrf = csrf_cookie(app.clone()).await;
    let body = if body.is_empty() {
        format!("csrf={csrf}")
    } else {
        format!("{body}&csrf={csrf}")
    };
    let mut builder = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/x-www-form-urlencoded")
        .header(header::COOKIE, format!("csrf={csrf}"));
    if htmx {
        builder = builder.header("HX-Request", "true");
    }
    request(app, builder.body(Body::from(body)).unwrap()).await
}

fn interval_html(label: &str) -> String {
    label.replace('&', "&#38;").replace('<', "&#60;")
}

#[tokio::test]
async fn about_shows_commit_identity_and_release_link() {
    let db = test_db().await;
    let (status, html) = get(app(&db), "/about").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("<h1>Flashcards</h1>"));
    assert!(html.contains(env!("CARGO_PKG_VERSION")));
    assert!(
        html.contains("/ataias/flashcards/commit/") || html.contains("unknown"),
        "expected commit link path or unknown, got {html}"
    );
    assert!(!html.contains("/commit/unknown"));
    assert!(html.contains("https://github.com/ataias/flashcards/releases"));
    assert!(
        html.contains("Uncompressed image size"),
        "expected uncompressed size label, got {html}"
    );
    // Local / CI have no pack-time size file; show unknown (not a MiB figure).
    assert!(
        html.contains("unknown"),
        "expected unknown commit and/or size without pack files, got {html}"
    );
}

#[tokio::test]
async fn index_lists_default_deck_and_local_static() {
    let db = test_db().await;
    let (status, html) = get(app(&db), "/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Default"));
    assert!(html.contains(&format!(
        "/decks/{}",
        db::list_decks(&db.pool).await.unwrap()[0].id
    )));
    assert!(html.contains("due 0"));
    assert!(html.contains("new 0"));
    assert!(html.contains(&format!(
        "/decks/{}/study",
        db::list_decks(&db.pool).await.unwrap()[0].id
    )));
    assert!(html.contains("hx-confirm"));
    assert!(!html.contains("No Decks yet"));
    assert!(html.contains("/static/htmx.min.js"));
    assert!(html.contains("/static/app.css"));
    assert!(hashed_asset(&html, "/static/htmx.min.js"));
    assert!(hashed_asset(&html, "/static/app.css"));
    assert!(hashed_asset(&html, "/static/perf-footer.js"));
    assert!(!html.contains("cdn.jsdelivr"));
    assert!(!html.contains("unpkg.com"));
    assert!(!html.contains("cdnjs"));
}

#[tokio::test]
async fn serves_vendored_htmx() {
    let db = test_db().await;
    let (status, js) = get(app(&db), "/static/htmx.min.js").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        js.len() > 10_000,
        "expected real minified HTMX, got {} bytes",
        js.len()
    );
    assert!(js.contains("htmx"));
    assert!(!js.contains("cdn.jsdelivr"));
}

#[tokio::test]
async fn serves_local_css() {
    let db = test_db().await;
    let (status, css) = get(app(&db), "/static/app.css").await;
    assert_eq!(status, StatusCode::OK);
    assert!(css.contains("body"));
}

#[tokio::test]
async fn static_files_ignore_content_hash_query() {
    let db = test_db().await;
    let (status, css) = get(app(&db), "/static/app.css?h=deadbeefdeadbeef").await;
    assert_eq!(status, StatusCode::OK);
    assert!(css.contains("body"));
    let (status, js) = get(app(&db), "/static/htmx.min.js?h=deadbeefdeadbeef").await;
    assert_eq!(status, StatusCode::OK);
    assert!(js.contains("htmx"));
    let (status, js) = get(app(&db), "/static/perf-footer.js?h=deadbeefdeadbeef").await;
    assert_eq!(status, StatusCode::OK);
    assert!(js.contains("page-perf"));
}

#[tokio::test]
async fn documents_are_no_store_and_static_is_immutable() {
    let db = test_db().await;
    let (status, headers, html) = request_parts(
        app(&db),
        Request::builder().uri("/").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cache_control(&headers), "no-store");
    assert!(hashed_asset(&html, "/static/app.css"));
    assert!(hashed_asset(&html, "/static/htmx.min.js"));
    assert!(hashed_asset(&html, "/static/perf-footer.js"));

    let (status, headers, _) = request_parts(
        app(&db),
        Request::builder()
            .uri("/static/app.css")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        cache_control(&headers),
        "public, max-age=31536000, immutable"
    );

    let (status, headers, _) = request_parts(
        app(&db),
        Request::builder()
            .uri("/static/htmx.min.js")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        cache_control(&headers),
        "public, max-age=31536000, immutable"
    );

    let (status, headers, _) = request_parts(
        app(&db),
        Request::builder()
            .uri("/static/perf-footer.js")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        cache_control(&headers),
        "public, max-age=31536000, immutable"
    );
}

#[tokio::test]
async fn full_pages_share_hashed_head_including_about() {
    let db = test_db().await;
    let deck_id = db::list_decks(&db.pool).await.unwrap()[0].id;
    let paths = [
        "/".to_string(),
        format!("/decks/{deck_id}"),
        format!("/decks/{deck_id}/study"),
        "/about".to_string(),
    ];
    for path in &paths {
        let (status, headers, html) = request_parts(
            app(&db),
            Request::builder().uri(path).body(Body::empty()).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert_eq!(cache_control(&headers), "no-store", "{path}");
        assert!(hashed_asset(&html, "/static/app.css"), "{path}");
        assert!(hashed_asset(&html, "/static/htmx.min.js"), "{path}");
        assert!(hashed_asset(&html, "/static/perf-footer.js"), "{path}");
        assert!(html.contains("<meta charset=\"utf-8\">"), "{path}");
        assert!(
            html.contains("name=\"viewport\""),
            "{path} must share viewport meta"
        );
    }
}

#[tokio::test]
async fn serves_timing_footer_script() {
    let db = test_db().await;
    let (status, js) = get(app(&db), "/static/perf-footer.js").await;
    assert_eq!(status, StatusCode::OK);
    assert!(js.contains("page-perf"));
    assert!(js.contains("htmx:afterRequest"));
    assert!(js.contains("Page loaded in"));
    assert!(js.contains("Updated in"));
}

#[tokio::test]
async fn every_page_has_blank_timing_footer_outside_main() {
    let db = test_db().await;
    let deck_id = db::list_decks(&db.pool).await.unwrap()[0].id;
    let (status, _) = post_form(
        app(&db),
        &format!("/decks/{deck_id}/cards"),
        "front=Capital+of+France&back=Paris",
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let paths = [
        "/".to_string(),
        format!("/decks/{deck_id}"),
        format!("/decks/{deck_id}/study"),
        "/about".to_string(),
    ];
    for path in &paths {
        let (status, html) = get(app(&db), path).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert!(
            hashed_asset(&html, "/static/perf-footer.js"),
            "{path} must load the hashed timing script"
        );
        // Blank until the browser measures; no server-rendered placeholder.
        assert!(
            html.contains("id=\"page-perf\"></footer>"),
            "{path} must render an empty footer, got {html}"
        );
        let footer = html.find("id=\"page-perf\"").unwrap();
        let main_end = html.find("</main>").expect("page must have a main");
        assert!(
            footer > main_end,
            "{path} footer must sit outside <main> so HTMX swaps cannot replace it"
        );
    }
}

#[tokio::test]
async fn htmx_fragments_omit_timing_footer() {
    let db = test_db().await;
    let deck_id = db::list_decks(&db.pool).await.unwrap()[0].id;
    let (status, decks) = post_form(app(&db), "/decks", "name=Spanish", true).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!decks.contains("page-perf"));
    assert!(!decks.contains("?h="));
    assert!(!decks.contains("/static/app.css"));
    assert!(!decks.contains("/static/htmx.min.js"));
    assert!(!decks.contains("/static/perf-footer.js"));

    let (status, cards) = post_form(
        app(&db),
        &format!("/decks/{deck_id}/cards"),
        "front=Capital+of+France&back=Paris",
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!cards.contains("page-perf"));

    let card_id = db::list_cards_in_deck(&db.pool, deck_id).await.unwrap()[0].id;
    let (status, review) = post_form(app(&db), &format!("/cards/{card_id}/reveal"), "", true).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!review.contains("page-perf"));
}

#[tokio::test]
async fn htmx_create_rename_delete_last_deck_leaves_empty() {
    let db = test_db().await;
    let pool = &db.pool;
    let (status, html) = post_form(app(&db), "/decks", "name=Spanish", true).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Spanish"));
    assert!(html.contains("Default"));
    assert!(html.contains("id=\"decks\""));

    let decks = db::list_decks(pool).await.unwrap();
    let spanish = decks.iter().find(|d| d.name == "Spanish").unwrap();
    let (status, html) = post_form(
        app(&db),
        &format!("/decks/{}/rename", spanish.id),
        "name=Italiano",
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Italiano"));
    assert!(!html.contains("Spanish"));

    let default_id = db::list_decks(pool)
        .await
        .unwrap()
        .into_iter()
        .find(|d| d.name == db::DEFAULT_DECK_NAME)
        .unwrap()
        .id;
    let italiano_id = db::list_decks(pool)
        .await
        .unwrap()
        .into_iter()
        .find(|d| d.name == "Italiano")
        .unwrap()
        .id;

    let (status, html) =
        post_form(app(&db), &format!("/decks/{default_id}/delete"), "", true).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Italiano"));
    assert!(!html.contains("Default"));

    let (status, html) =
        post_form(app(&db), &format!("/decks/{italiano_id}/delete"), "", true).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!html.contains("Italiano"));
    assert!(!html.contains("Default"));
    assert!(!html.contains("recreate"));
    assert!(html.contains("New Deck"));
    assert!(html.contains("No Decks yet. Create one to add Cards or study."));
    assert!(html.contains("class=\"create-deck\""));
    assert!(db::list_decks(pool).await.unwrap().is_empty());

    let (status, html) = post_form(app(&db), "/decks", "name=Japanese", true).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Japanese"));
    assert!(!html.contains("No Decks yet"));
    assert!(!html.contains("Default"));
    assert!(html.contains("due 0"));
    assert!(html.contains("new 0"));
}

#[tokio::test]
async fn home_empty_when_zero_decks_shows_create_form() {
    let db = test_db().await;
    let deck_id = db::list_decks(&db.pool).await.unwrap()[0].id;
    db::delete_deck(&db.pool, deck_id).await.unwrap();

    let (status, html) = get(app(&db), "/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("No Decks yet. Create one to add Cards or study."));
    assert!(html.contains("New Deck"));
    assert!(html.contains("class=\"create-deck\""));
    assert!(html.contains("hx-post=\"/decks\""));
    assert!(!html.contains("Default"));
    assert!(!html.contains("recreate"));
    assert!(!html.contains("class=\"deck-list\""));
}

#[tokio::test]
async fn create_rejects_empty_name() {
    let db = test_db().await;
    let (status, html) = post_form(app(&db), "/decks", "name=+++", true).await;
    // "+" is form-urlencoded space; three pluses → whitespace-only.
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Deck name cannot be empty"));
    assert!(html.contains("Default"));
}

#[tokio::test]
async fn non_htmx_create_redirects_home() {
    let db = test_db().await;
    let (status, _) = post_form(app(&db), "/decks", "name=French", false).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let (status, html) = get(app(&db), "/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("French"));
}

#[tokio::test]
async fn deck_page_lists_cards_empty_then_crud() {
    let db = test_db().await;
    let pool = &db.pool;
    let deck_id = db::list_decks(pool).await.unwrap()[0].id;

    let (status, html) = get(app(&db), &format!("/decks/{deck_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Default"));
    assert!(html.contains("No Cards in this Deck yet"));
    assert!(html.contains("id=\"cards\""));
    assert!(html.contains(&format!("/decks/{deck_id}/study")));

    let (status, html) = post_form(
        app(&db),
        &format!("/decks/{deck_id}/cards"),
        "front=Capital+of+France&back=Paris",
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Capital of France"));
    assert!(html.contains("Paris"));
    assert!(html.contains("id=\"cards\""));
    assert!(!html.contains("No Cards in this Deck yet"));

    let card_id = db::list_cards_in_deck(pool, deck_id).await.unwrap()[0].id;
    let (status, html) = post_form(
        app(&db),
        &format!("/cards/{card_id}"),
        "front=Capital+of+Italy&back=Rome",
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Capital of Italy"));
    assert!(html.contains("Rome"));
    assert!(!html.contains("France"));
    assert!(!html.contains("Paris"));

    let (status, html) = post_form(app(&db), &format!("/cards/{card_id}/delete"), "", true).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("No Cards in this Deck yet"));
    assert!(!html.contains("Rome"));
    assert!(
        db::list_cards_in_deck(pool, deck_id)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn deck_page_shows_phase_and_due() {
    let db = test_db().await;
    let pool = &db.pool;
    let deck_id = db::list_decks(pool).await.unwrap()[0].id;

    let new_card = db::create_card(pool, deck_id, "New front", "New back")
        .await
        .unwrap();
    let due_review = db::create_card(pool, deck_id, "Due front", "Due back")
        .await
        .unwrap();
    let later_review = db::create_card(pool, deck_id, "Later front", "Later back")
        .await
        .unwrap();

    let now = chrono::Utc::now();
    set_review(pool, due_review.id, now - chrono::Duration::hours(1)).await;
    set_review(
        pool,
        later_review.id,
        now + chrono::Duration::days(10) + chrono::Duration::hours(1),
    )
    .await;

    let listed = db::list_card_text_in_deck(pool, deck_id).await.unwrap();
    assert_eq!(
        listed.iter().map(|card| card.id).collect::<Vec<_>>(),
        vec![new_card.id, due_review.id, later_review.id]
    );
    assert_eq!(listed[0].phase, db::Phase::New);
    assert_eq!(listed[1].phase, db::Phase::Review);
    assert_eq!(listed[2].phase, db::Phase::Review);

    let (status, html) = get(app(&db), &format!("/decks/{deck_id}")).await;
    assert_eq!(status, StatusCode::OK);

    let new_pos = html.find("New front").expect("new card front");
    let due_pos = html.find("Due front").expect("due review front");
    let later_pos = html.find("Later front").expect("later review front");
    assert!(new_pos < due_pos && due_pos < later_pos, "id order");

    let new_block = &html[new_pos..due_pos];
    assert!(new_block.contains(r#"<span class="card-phase">New</span>"#));
    assert!(!new_block.contains("card-due"));
    assert!(!new_block.contains("due now"));
    assert!(!new_block.contains("due in"));

    let due_block = &html[due_pos..later_pos];
    assert!(due_block.contains(r#"<span class="card-phase">Review</span>"#));
    assert!(due_block.contains(r#"<span class="card-due">due now</span>"#));

    let later_due = listed[2]
        .due_label(chrono::Utc::now())
        .expect("later review due label");
    let later_block = &html[later_pos..];
    assert!(later_block.contains(r#"<span class="card-phase">Review</span>"#));
    assert!(later_block.contains(&format!(r#"<span class="card-due">{later_due}</span>"#)));
}

#[tokio::test]
async fn create_card_rejects_empty_sides() {
    let db = test_db().await;
    let deck_id = db::list_decks(&db.pool).await.unwrap()[0].id;
    let (status, html) = post_form(
        app(&db),
        &format!("/decks/{deck_id}/cards"),
        "front=+++&back=Paris",
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Card front cannot be empty"));
    assert!(
        db::list_cards_in_deck(&db.pool, deck_id)
            .await
            .unwrap()
            .is_empty()
    );

    let (status, html) = post_form(
        app(&db),
        &format!("/decks/{deck_id}/cards"),
        "front=Q&back=+++",
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Card back cannot be empty"));
    assert!(html.contains(">Q</textarea>"));
}

#[tokio::test]
async fn create_card_preserves_draft_on_empty_front() {
    let db = test_db().await;
    let deck_id = db::list_decks(&db.pool).await.unwrap()[0].id;
    let (status, html) = post_form(
        app(&db),
        &format!("/decks/{deck_id}/cards"),
        "front=+++&back=Paris",
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Card front cannot be empty"));
    assert!(html.contains(">Paris</textarea>"));
    assert!(html.contains("class=\"create-card\""));
}

#[tokio::test]
async fn update_card_rejects_empty_sides_and_preserves_draft() {
    let db = test_db().await;
    let deck_id = db::list_decks(&db.pool).await.unwrap()[0].id;
    let card = db::create_card(&db.pool, deck_id, "Q", "A").await.unwrap();
    let (status, html) = post_form(
        app(&db),
        &format!("/cards/{}", card.id),
        "front=+++&back=Kept+draft",
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Card front cannot be empty"));
    assert!(html.contains(">Kept draft</textarea>"));
    assert!(html.contains(">Q</p>"));
    let stored = db::get_card(&db.pool, card.id).await.unwrap().unwrap();
    assert_eq!(stored.front, "Q");
    assert_eq!(stored.back, "A");

    let (status, html) = post_form(
        app(&db),
        &format!("/cards/{}", card.id),
        "front=Kept+front&back=+++",
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Card back cannot be empty"));
    assert!(html.contains(">Kept front</textarea>"));
}

#[tokio::test]
async fn non_htmx_update_and_delete_card_redirect_to_deck() {
    let db = test_db().await;
    let deck_id = db::list_decks(&db.pool).await.unwrap()[0].id;
    let card = db::create_card(&db.pool, deck_id, "Q", "A").await.unwrap();
    let (status, _) = post_form(
        app(&db),
        &format!("/cards/{}", card.id),
        "front=Q2&back=A2",
        false,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let (status, html) = get(app(&db), &format!("/decks/{deck_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains(">Q2</p>"));
    assert!(html.contains(">A2</p>"));

    let (status, _) = post_form(app(&db), &format!("/cards/{}/delete", card.id), "", false).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let (status, html) = get(app(&db), &format!("/decks/{deck_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("No Cards in this Deck yet"));
}

#[tokio::test]
async fn home_counts_new_card_after_create() {
    let db = test_db().await;
    let deck_id = db::list_decks(&db.pool).await.unwrap()[0].id;
    let (status, _) = post_form(
        app(&db),
        &format!("/decks/{deck_id}/cards"),
        "front=Q&back=A",
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, html) = get(app(&db), "/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("due 0"));
    assert!(html.contains("new 1"));
}

#[tokio::test]
async fn non_htmx_create_card_redirects_to_deck() {
    let db = test_db().await;
    let deck_id = db::list_decks(&db.pool).await.unwrap()[0].id;
    let (status, _) = post_form(
        app(&db),
        &format!("/decks/{deck_id}/cards"),
        "front=Q&back=A",
        false,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let (status, html) = get(app(&db), &format!("/decks/{deck_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains(">Q</p>"));
    assert!(html.contains(">A</p>"));
}

#[tokio::test]
async fn missing_deck_or_card_is_not_found() {
    let db = test_db().await;
    let (status, _) = get(app(&db), "/decks/999").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = post_form(app(&db), "/decks/999/delete", "", true).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = post_form(app(&db), "/decks/999/cards", "front=Q&back=A", true).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = post_form(app(&db), "/cards/999", "front=Q&back=A", true).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = post_form(app(&db), "/cards/999/delete", "", true).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = get(app(&db), "/decks/999/study").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = post_form(app(&db), "/cards/999/reveal", "", true).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = post_form(app(&db), "/cards/999/rate", "rating=3", true).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn card_text_is_html_escaped() {
    let db = test_db().await;
    let deck_id = db::list_decks(&db.pool).await.unwrap()[0].id;
    db::create_card(&db.pool, deck_id, "<script>alert(1)</script>", "a&b")
        .await
        .unwrap();
    let (status, html) = get(app(&db), &format!("/decks/{deck_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!html.contains("<script>alert(1)</script>"));
    assert!(html.contains("&#60;script&#62;alert(1)&#60;/script&#62;"));
    assert!(html.contains("a&#38;b"));
}

#[tokio::test]
async fn study_empty_when_no_cards() {
    let db = test_db().await;
    let deck_id = db::list_decks(&db.pool).await.unwrap()[0].id;
    let (status, html) = get(app(&db), &format!("/decks/{deck_id}/study")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("No Cards to Review"));
    assert!(html.contains("<h1>Study</h1>"));
    assert!(html.contains(&format!("/decks/{deck_id}")));
    assert!(!html.contains("Show answer"));
}

#[tokio::test]
async fn study_shows_front_hides_back() {
    let db = test_db().await;
    let deck_id = db::list_decks(&db.pool).await.unwrap()[0].id;
    let card = db::create_card(&db.pool, deck_id, "Capital of France", "Paris")
        .await
        .unwrap();
    let (status, html) = get(app(&db), &format!("/decks/{deck_id}/study")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Capital of France"));
    assert!(!html.contains("Paris"));
    assert!(html.contains("Show answer"));
    assert!(html.contains(r#"method="post""#));
    assert!(html.contains(&format!(r#"action="/cards/{}/reveal""#, card.id)));
    assert!(!html.contains("Again"));
}

#[tokio::test]
async fn reveal_shows_back_and_ratings() {
    let db = test_db().await;
    let deck_id = db::list_decks(&db.pool).await.unwrap()[0].id;
    let card = db::create_card(&db.pool, deck_id, "Q", "Secret answer")
        .await
        .unwrap();
    let (status, html) = post_form(app(&db), &format!("/cards/{}/reveal", card.id), "", true).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("id=\"study\""));
    assert!(html.contains(">Q</p>"));
    assert!(html.contains(">Secret answer</p>"));
    assert!(html.contains("Again"));
    assert!(html.contains("Hard"));
    assert!(html.contains("Good"));
    assert!(html.contains("Easy"));
    assert!(html.contains(r#"method="post""#));
    assert!(html.contains(&format!(r#"action="/cards/{}/rate""#, card.id)));
}

#[tokio::test]
async fn reveal_shows_interval_previews() {
    let db = test_db().await;
    let pool = &db.pool;
    let deck_id = db::list_decks(pool).await.unwrap()[0].id;
    let card = db::create_card(pool, deck_id, "Q", "A").await.unwrap();
    let now = chrono::Utc::now();
    let again = card.preview_interval_label(db::Rating::Again, now).unwrap();
    let hard = card.preview_interval_label(db::Rating::Hard, now).unwrap();
    let good = card.preview_interval_label(db::Rating::Good, now).unwrap();
    let easy = card.preview_interval_label(db::Rating::Easy, now).unwrap();

    let (status, html) = post_form(app(&db), &format!("/cards/{}/reveal", card.id), "", true).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains(&interval_html(&again)));
    assert!(html.contains(&interval_html(&hard)));
    assert!(html.contains(&interval_html(&good)));
    assert!(html.contains(&interval_html(&easy)));
    assert!(html.contains(&format!(
        "Again <span class=\"interval\">{}</span>",
        interval_html(&again)
    )));
    assert!(html.contains(&format!(
        "Hard <span class=\"interval\">{}</span>",
        interval_html(&hard)
    )));
    assert!(html.contains(&format!(
        "Good <span class=\"interval\">{}</span>",
        interval_html(&good)
    )));
    assert!(html.contains(&format!(
        "Easy <span class=\"interval\">{}</span>",
        interval_html(&easy)
    )));

    let stored = db::get_card(pool, card.id).await.unwrap().unwrap();
    assert_eq!(stored, card);
    assert!(review_ratings(pool, card.id).await.is_empty());
}

#[tokio::test]
async fn rate_persists_and_htmx_advances_then_done() {
    let db = test_db().await;
    let pool = &db.pool;
    let deck_id = db::list_decks(pool).await.unwrap()[0].id;
    let first = db::create_card(pool, deck_id, "Q1", "A1").await.unwrap();
    let second = db::create_card(pool, deck_id, "Q2", "A2").await.unwrap();

    let (status, html) = post_form(
        app(&db),
        &format!("/cards/{}/rate", first.id),
        "rating=3",
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Q2"));
    assert!(!html.contains("A2"));
    assert!(!html.contains("Q1"));
    assert!(html.contains("Show answer"));

    let stored = db::get_card(pool, first.id).await.unwrap().unwrap();
    assert!(!stored.is_new());
    assert!(stored.due.unwrap() > chrono::Utc::now());

    let (status, html) = post_form(
        app(&db),
        &format!("/cards/{}/rate", second.id),
        "rating=4",
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("No Cards to Review"));
    assert!(!html.contains("Q2"));
    let stored = db::get_card(pool, second.id).await.unwrap().unwrap();
    assert!(!stored.is_new());
    assert!(stored.due.unwrap() > chrono::Utc::now());
}

#[tokio::test]
async fn study_respects_new_card_cap() {
    let db = test_db().await;
    let pool = &db.pool;
    let deck_id = db::list_decks(pool).await.unwrap()[0].id;
    let now = chrono::Utc::now();
    for i in 0..21 {
        let card = db::create_card(pool, deck_id, &format!("Q{i}"), &format!("A{i}"))
            .await
            .unwrap();
        if i < 20 {
            db::rate_card(pool, &card, db::Rating::Good, now)
                .await
                .unwrap();
        }
    }
    let (status, html) = get(app(&db), &format!("/decks/{deck_id}/study")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("No Cards to Review"));
    assert!(!html.contains("Q20"));
}

#[tokio::test]
async fn invalid_rating_keeps_revealed_card() {
    let db = test_db().await;
    let deck_id = db::list_decks(&db.pool).await.unwrap()[0].id;
    let card = db::create_card(&db.pool, deck_id, "Q", "A").await.unwrap();
    let (status, html) = post_form(
        app(&db),
        &format!("/cards/{}/rate", card.id),
        "rating=9",
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Choose Again, Hard, Good, or Easy"));
    assert!(html.contains(">Q</p>"));
    assert!(html.contains(">A</p>"));
    let stored = db::get_card(&db.pool, card.id).await.unwrap().unwrap();
    assert!(stored.is_new());
}

#[tokio::test]
async fn non_htmx_rate_redirects_to_study_done() {
    let db = test_db().await;
    let deck_id = db::list_decks(&db.pool).await.unwrap()[0].id;
    let card = db::create_card(&db.pool, deck_id, "Q", "A").await.unwrap();
    let (status, _) = post_form(
        app(&db),
        &format!("/cards/{}/rate", card.id),
        "rating=1",
        false,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let (status, html) = get(app(&db), &format!("/decks/{deck_id}/study")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("No Cards to Review"));
    let stored = db::get_card(&db.pool, card.id).await.unwrap().unwrap();
    assert!(!stored.is_new());
    assert!(stored.due.unwrap() > chrono::Utc::now());
}

#[tokio::test]
async fn study_stays_inside_the_requested_deck() {
    let db = test_db().await;
    let pool = &db.pool;
    let default_id = db::list_decks(pool).await.unwrap()[0].id;
    let other = owned_deck(&db, "Spanish").await;
    db::create_card(pool, default_id, "Default front", "Default back")
        .await
        .unwrap();
    db::create_card(pool, other.id, "Spanish front", "Spanish back")
        .await
        .unwrap();

    let (status, html) = get(app(&db), &format!("/decks/{default_id}/study")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Default front"));
    assert!(!html.contains("Spanish front"));

    let (status, html) = get(app(&db), &format!("/decks/{}/study", other.id)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Spanish front"));
    assert!(!html.contains("Default front"));
}

#[tokio::test]
async fn study_card_text_is_html_escaped() {
    let db = test_db().await;
    let deck_id = db::list_decks(&db.pool).await.unwrap()[0].id;
    let card = db::create_card(&db.pool, deck_id, "<script>alert(1)</script>", "a&b")
        .await
        .unwrap();
    let (status, html) = get(app(&db), &format!("/decks/{deck_id}/study")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!html.contains("<script>alert(1)</script>"));
    assert!(html.contains("&#60;script&#62;alert(1)&#60;/script&#62;"));

    let (status, html) = post_form(app(&db), &format!("/cards/{}/reveal", card.id), "", true).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!html.contains("<script>alert(1)</script>"));
    assert!(html.contains("a&#38;b"));
}

#[tokio::test]
async fn new_card_ratings_follow_learning_steps() {
    let db = test_db().await;
    let pool = &db.pool;
    let deck_id = db::list_decks(pool).await.unwrap()[0].id;
    let card = db::create_card(pool, deck_id, "Q", "A").await.unwrap();

    let after_again = rate_matches_domain(&db, card.id, db::Rating::Again).await;
    assert_eq!(after_again.phase, db::Phase::Learning);
    assert_eq!(after_again.learning_step, Some(0));
    assert_eq!(after_again.memory, None);
    assert_eq!(review_ratings(pool, card.id).await, vec![1]);

    let (status, html) = get(app(&db), &format!("/decks/{deck_id}/study")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("No Cards to Review"));
    assert!(!html.contains(">Q</p>"));

    let (status, html) = get(app(&db), "/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("due 0"));
    assert!(html.contains("new 0"));

    let after_hard = rate_matches_domain(&db, card.id, db::Rating::Hard).await;
    assert_eq!(after_hard.phase, db::Phase::Learning);
    assert_eq!(after_hard.learning_step, Some(0));
    assert_eq!(after_hard.memory, None);

    let after_good = rate_matches_domain(&db, card.id, db::Rating::Good).await;
    assert_eq!(after_good.phase, db::Phase::Learning);
    assert_eq!(after_good.learning_step, Some(1));
    assert_eq!(after_good.memory, None);
    assert_eq!(review_ratings(pool, card.id).await, vec![1, 2, 3]);
}

#[tokio::test]
async fn learning_good_on_last_step_graduates() {
    let db = test_db().await;
    let pool = &db.pool;
    let deck_id = db::list_decks(pool).await.unwrap()[0].id;
    let card = db::create_card(pool, deck_id, "Q", "A").await.unwrap();

    let learning = rate_matches_domain(&db, card.id, db::Rating::Good).await;
    assert_eq!(learning.phase, db::Phase::Learning);
    assert_eq!(learning.learning_step, Some(1));

    let graduated = rate_matches_domain(&db, card.id, db::Rating::Good).await;
    assert_eq!(graduated.phase, db::Phase::Review);
    assert_eq!(graduated.learning_step, None);
    assert!(graduated.memory.is_some());
    assert_eq!(review_ratings(pool, card.id).await, vec![3, 3]);

    let early = db::create_card(pool, deck_id, "Early", "A").await.unwrap();
    let learning = rate_matches_domain(&db, early.id, db::Rating::Again).await;
    assert_eq!(learning.phase, db::Phase::Learning);
    let easy = rate_matches_domain(&db, early.id, db::Rating::Easy).await;
    assert_eq!(easy.phase, db::Phase::Review);
    assert_eq!(easy.learning_step, None);
    assert!(easy.memory.is_some());
}

#[tokio::test]
async fn new_easy_graduates_to_review() {
    let db = test_db().await;
    let pool = &db.pool;
    let deck_id = db::list_decks(pool).await.unwrap()[0].id;
    let card = db::create_card(pool, deck_id, "Q", "A").await.unwrap();

    let graduated = rate_matches_domain(&db, card.id, db::Rating::Easy).await;
    assert_eq!(graduated.phase, db::Phase::Review);
    assert_eq!(graduated.learning_step, None);
    assert!(graduated.memory.is_some());
    assert_eq!(review_ratings(pool, card.id).await, vec![4]);

    let (status, html) = get(app(&db), &format!("/decks/{deck_id}/study")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("No Cards to Review"));

    let (status, html) = get(app(&db), "/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("due 0"));
    assert!(html.contains("new 0"));
}

#[tokio::test]
async fn review_again_enters_ten_minute_relearning() {
    let db = test_db().await;
    let pool = &db.pool;
    let deck_id = db::list_decks(pool).await.unwrap()[0].id;
    let card = db::create_card(pool, deck_id, "Q", "A").await.unwrap();

    let review = rate_matches_domain(&db, card.id, db::Rating::Easy).await;
    assert_eq!(review.phase, db::Phase::Review);
    let review_memory = review.memory.expect("Easy from New must set FSRS memory");

    let relearning = rate_matches_domain(&db, card.id, db::Rating::Again).await;
    assert_eq!(relearning.phase, db::Phase::Relearning);
    assert_eq!(relearning.learning_step, Some(0));
    let relearning_memory = relearning
        .memory
        .expect("Review Again updates FSRS immediately");
    assert_ne!(relearning_memory, review_memory);
    assert_eq!(
        relearning.due,
        Some(relearning.last_review.unwrap() + chrono::Duration::minutes(10))
    );

    let (status, html) = get(app(&db), &format!("/decks/{deck_id}/study")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("No Cards to Review"));

    let graduated = rate_matches_domain(&db, card.id, db::Rating::Good).await;
    assert_eq!(graduated.phase, db::Phase::Review);
    assert_eq!(graduated.learning_step, None);
    assert!(graduated.memory.is_some());
    assert_eq!(review_ratings(pool, card.id).await, vec![4, 1, 3]);
}

#[tokio::test]
async fn learning_card_reappears_when_step_elapses() {
    let db = test_db().await;
    let pool = &db.pool;
    let deck_id = db::list_decks(pool).await.unwrap()[0].id;
    let card = db::create_card(pool, deck_id, "Step due", "A")
        .await
        .unwrap();

    let learning = rate_matches_domain(&db, card.id, db::Rating::Again).await;
    assert_eq!(learning.phase, db::Phase::Learning);
    assert!(learning.due.unwrap() > chrono::Utc::now());

    let (status, html) = get(app(&db), &format!("/decks/{deck_id}/study")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("No Cards to Review"));
    assert!(!html.contains("Step due"));

    set_due(pool, card.id, chrono::Utc::now()).await;

    let (status, html) = get(app(&db), &format!("/decks/{deck_id}/study")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Step due"));
    assert!(html.contains("Show answer"));
    assert!(!html.contains("No Cards to Review"));

    let (status, html) = get(app(&db), "/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("due 1"));
    assert!(html.contains("new 0"));
}

#[tokio::test]
async fn new_cap_consumed_on_enter_learning_including_easy_from_new() {
    let db = test_db().await;
    let pool = &db.pool;
    let learning_id = db::list_decks(pool).await.unwrap()[0].id;
    for i in 0..21 {
        db::create_card(pool, learning_id, &format!("L{i}"), &format!("A{i}"))
            .await
            .unwrap();
    }
    let learning_cards = db::list_cards_in_deck(pool, learning_id).await.unwrap();
    for card in learning_cards.iter().take(20) {
        rate_matches_domain(&db, card.id, db::Rating::Again).await;
    }

    let (status, html) = get(app(&db), &format!("/decks/{learning_id}/study")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("No Cards to Review"));
    assert!(!html.contains("L20"));

    let (status, html) = get(app(&db), "/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("due 0"));
    assert!(html.contains("new 0"));

    let easy_deck = owned_deck(&db, "EasyCap").await;
    for i in 0..21 {
        db::create_card(pool, easy_deck.id, &format!("E{i}"), &format!("A{i}"))
            .await
            .unwrap();
    }
    let easy_cards = db::list_cards_in_deck(pool, easy_deck.id).await.unwrap();
    for card in easy_cards.iter().take(20) {
        rate_matches_domain(&db, card.id, db::Rating::Easy).await;
    }

    let (status, html) = get(app(&db), &format!("/decks/{}/study", easy_deck.id)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("No Cards to Review"));
    assert!(!html.contains("E20"));

    let leftover = db::create_card(pool, learning_id, "Further", "A")
        .await
        .unwrap();
    let first = &learning_cards[0];
    rate_matches_domain(&db, first.id, db::Rating::Hard).await;
    assert_eq!(review_ratings(pool, first.id).await.len(), 2);

    let (status, html) = get(app(&db), &format!("/decks/{learning_id}/study")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("No Cards to Review"));
    assert!(!html.contains("L20"));
    assert!(!html.contains("Further"));
    let leftover = db::get_card(pool, leftover.id).await.unwrap().unwrap();
    assert!(leftover.is_new());
}

#[tokio::test]
async fn review_good_due_can_be_shorter_than_one_day() {
    let db = test_db().await;
    let pool = &db.pool;
    let deck_id = db::list_decks(pool).await.unwrap()[0].id;
    let card = db::create_card(pool, deck_id, "Sub-day", "A")
        .await
        .unwrap();
    let last = chrono::Utc::now() - chrono::Duration::days(1);
    let due = chrono::Utc::now();
    sqlx::query(
        "UPDATE cards
         SET phase = 'review', learning_step = NULL,
             stability = 0.1, difficulty = 10.0, due = ?, last_review = ?
         WHERE id = ?",
    )
    .bind(due.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
    .bind(last.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
    .bind(card.id)
    .execute(pool)
    .await
    .unwrap();

    let rated = rate_matches_domain(&db, card.id, db::Rating::Good).await;
    assert_eq!(rated.phase, db::Phase::Review);
    let rated_at = rated.last_review.unwrap();
    let due = rated.due.unwrap();
    assert!(due > rated_at);
    assert!(
        due < rated_at + chrono::Duration::days(1),
        "unfloored FSRS Review due should be able to land before one day, got {due}"
    );
}

async fn empty_db() -> TestDb {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("flashcards.db");
    let pool = db::open(&path).await.unwrap();
    TestDb {
        store: SqliteStore::new(pool.clone()),
        pool,
        user_id: 0,
        _dir: dir,
    }
}

#[tokio::test]
async fn empty_db_redirects_to_bootstrap_and_keeps_about_public() {
    let db = empty_db().await;
    let (status, headers, _) = request_parts(
        app(&db),
        Request::builder().uri("/").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers[header::LOCATION], "/bootstrap");

    let (status, headers, _) = request_parts(
        app(&db),
        Request::builder()
            .uri("/decks/1/study")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers[header::LOCATION], "/bootstrap");

    let (status, html) = get(app(&db), "/about").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("<h1>Flashcards</h1>"));
    assert!(html.contains("About"));
}

async fn login_attempt(app: Router, csrf: &str, password: &str) -> (StatusCode, HeaderMap, String) {
    request_parts(
        app,
        Request::builder()
            .method("POST")
            .uri("/login")
            .header("content-type", "application/x-www-form-urlencoded")
            .header(header::COOKIE, format!("csrf={csrf}"))
            .body(Body::from(format!(
                "username=admin&password={password}&csrf={csrf}"
            )))
            .unwrap(),
    )
    .await
}

#[tokio::test]
async fn csrf_rejects_bad_token() {
    let db = test_db().await;
    let (status, headers, html) = request_parts(
        app(&db),
        Request::builder().uri("/").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let csrf = extract_csrf(&html);
    let cookie = set_cookie_value(&headers, "csrf").expect("csrf cookie");
    assert_eq!(csrf, cookie);
    let (status, html) = request(
        app(&db),
        Request::builder()
            .method("POST")
            .uri("/decks")
            .header("content-type", "application/x-www-form-urlencoded")
            .header(header::COOKIE, format!("csrf={cookie}"))
            .body(Body::from("name=Nope&csrf=not-the-token"))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(html.contains("Invalid CSRF token"));
    assert!(
        db::list_decks(&db.pool)
            .await
            .unwrap()
            .iter()
            .all(|deck| deck.name != "Nope")
    );
}

#[tokio::test]
async fn login_rate_limit_trips() {
    let db = test_db().await;
    let router = app(&db);
    let csrf = csrf_cookie(router.clone()).await;
    for i in 0..5 {
        let (status, _, html) = login_attempt(router.clone(), &csrf, "wrong").await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "attempt {i}");
        assert!(html.contains("Invalid username or password"));
    }
    let (status, _, html) = login_attempt(router, &csrf, "wrong").await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(html.contains("Too many attempts"));
}

#[tokio::test]
async fn login_sets_session_cookie_without_secure_on_loopback() {
    let db = test_db().await;
    let csrf = csrf_cookie(app(&db)).await;
    let (status, headers, _) = login_attempt(app(&db), &csrf, "secret").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers[header::LOCATION], "/");
    let session = set_cookie_value(&headers, "session").expect("session cookie");
    let set_cookie = headers[header::SET_COOKIE].to_str().unwrap();
    assert!(set_cookie.contains("HttpOnly") && set_cookie.contains("SameSite=Strict"));
    assert!(
        !set_cookie.contains("Secure"),
        "loopback tests omit Secure: {set_cookie}"
    );

    let (status, html) = request(
        app(&db),
        Request::builder()
            .uri("/")
            .header(header::COOKIE, format!("session={session}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Default"));
}
