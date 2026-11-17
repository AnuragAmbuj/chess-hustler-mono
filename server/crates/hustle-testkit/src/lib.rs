//! hustle-testkit — shared L3 integration-test harness (dev-dependency only).
//!
//! Skips gracefully when `CH_TEST_DATABASE_URL` / `CH_TEST_REDIS_URL` are
//! unset so `cargo test` stays green on machines without services; CI sets
//! them (Postgres/Redis service containers). Each test binary gets its own
//! uniquely-named Postgres database with `migrations/*.sql` applied once.

use anyhow::Context;
use sqlx::postgres::{PgPool, PgPoolOptions};
use tokio::sync::OnceCell;

static PG_CELL: OnceCell<Option<String>> = OnceCell::const_new();

/// Per-test PgPool bound to the CURRENT tokio runtime (a sqlx pool must not
/// cross runtimes — background tasks are spawned on the creating runtime).
/// The database itself is created once per process and shared.
pub async fn pool() -> Option<PgPool> {
    let url = shared_db().await?;
    PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .ok()
}

async fn shared_db() -> Option<String> {
    PG_CELL
        .get_or_init(async || {
            let url = std::env::var("CH_TEST_DATABASE_URL").ok()?;
            Some(setup_db(&url).await.expect("test database setup"))
        })
        .await
        .clone()
}

async fn setup_db(url: &str) -> anyhow::Result<String> {
    // `postgres://user@host:port/db` → server URL + fresh unique db name.
    let (server, _) = url
        .rsplit_once('/')
        .context("CH_TEST_DATABASE_URL must contain a database segment")?;
    let unique = format!("t{}_{}", ulid::Ulid::new(), std::process::id())
        .to_lowercase()
        .replace(['-', '_'], "_");

    let admin = PgPoolOptions::new()
        .max_connections(2)
        .connect(server)
        .await?;
    sqlx::query(format!("CREATE DATABASE {unique}").as_str())
        .execute(&admin)
        .await?;
    admin.close().await;

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(format!("{server}/{unique}").as_str())
        .await
        .with_context(|| format!("connecting to test db {unique}"))?;
    apply_migrations(&pool).await?;
    pool.close().await;
    Ok(format!("{server}/{unique}"))
}

/// Executes every `migrations/*.sql` (sorted) inside one transaction.
async fn apply_migrations(pool: &PgPool) -> anyhow::Result<()> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../migrations");
    let mut paths: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "sql"))
        .collect();
    paths.sort();
    let mut tx = pool.begin().await?;
    for path in paths {
        let sql = std::fs::read_to_string(&path)?;
        sqlx::raw_sql(&sql).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Insert a user (+ profile) and return its id. Gold balance is NOT created —
/// tests control it explicitly.
pub async fn make_user(pool: &PgPool, tag: &str) -> i64 {
    static USERS: OnceCell<std::sync::atomic::AtomicU64> = OnceCell::const_new();
    let counter = USERS
        .get_or_init(|| async { std::sync::atomic::AtomicU64::new(0) })
        .await
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let phone = format!("+9199{tag}{counter:05}");
    let handle = format!("u_{tag}_{counter}").to_lowercase();
    let display = format!("Test {tag} {counter}");
    sqlx::query_scalar(
        "WITH u AS (
           INSERT INTO users (phone_e164, age_confirmed_at) VALUES ($1, now()) RETURNING id
         ), p AS (
           INSERT INTO profiles (user_id, handle, display_name)
           SELECT id, $2, $3 FROM u RETURNING user_id
         )
         SELECT user_id FROM p",
    )
    .bind(phone)
    .bind(&handle)
    .bind(display)
    .fetch_one(pool)
    .await
    .expect("make_user")
}

pub async fn make_two_users(pool: &PgPool, tag: &str) -> (i64, i64) {
    let a = make_user(pool, tag).await;
    let b = make_user(pool, tag).await;
    (a, b)
}

/// Insert an active rated game between two users; returns its id.
pub async fn make_game(pool: &PgPool, white: i64, black: i64, pool_name: &str) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO games (public_id, white_user_id, black_user_id, pool, time_control,
                             rated, status, fen_start, fen_current)
         VALUES ($1, $2, $3, $4, '{\"initial_ms\":180000,\"increment_ms\":2000}'::jsonb,
                 true, 'active', $5, $5)
         RETURNING id",
    )
    .bind(ulid::Ulid::new().to_string())
    .bind(white)
    .bind(black)
    .bind(pool_name)
    .bind("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
    .fetch_one(pool)
    .await
    .expect("make_game")
}

// ---- Redis ------------------------------------------------------------------

static RD_CELL: OnceCell<Option<redis::Client>> = OnceCell::const_new();

/// Shared Redis client (flushed once per process), or `None` when unconfigured.
pub async fn redis() -> Option<redis::Client> {
    RD_CELL
        .get_or_init(async || {
            let url = std::env::var("CH_TEST_REDIS_URL").ok()?;
            let client = redis::Client::open(url).ok()?;
            let mut conn = client.get_multiplexed_async_connection().await.ok()?;
            let _: Result<i64, _> = redis::AsyncCommands::flushdb(&mut conn).await;
            Some(client)
        })
        .await
        .clone()
}
