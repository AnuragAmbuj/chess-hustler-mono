//! L3 Integration Tests for Game Orchestration, Moves, and REST/WS Endpoints.
//! Real-flow testing against live PostgreSQL and Redis.

use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use hustle_api::{router, AppState};
use hustle_chess::GameServiceError;
use hustle_domain::{Pool, TimeControl};
use hustle_identity::signup::{Consents, ProfileInput, VerifiedIdentity};
use hustle_kernel::ServerConfig;
use hustle_realtime::{read_frame, write_client_frame, ClientCommand, Envelope, Frame};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tower::ServiceExt;

fn counter() -> u64 {
    static N: AtomicU64 = AtomicU64::new(1000);
    N.fetch_add(1, Ordering::Relaxed)
}

fn phone() -> String {
    format!("+9196{:010}", counter())
}

fn consents() -> Consents {
    Consents {
        essential: true,
        functional: true,
        marketing: false,
        personalised_ads: false,
        chat: true,
    }
}

async fn create_test_player(state: &AppState, prefix: &str) -> (String, String, i64) {
    let p = phone();
    let h = format!("{prefix}_{}", counter());
    let outcome = state
        .signup
        .signup(
            VerifiedIdentity::Phone(p),
            consents(),
            ProfileInput {
                handle: h.clone(),
                display_name: format!("Player {h}"),
            },
            None,
            Some("127.0.0.1"),
        )
        .await
        .expect("signup ok");
    (outcome.access_token, h, outcome.user_id)
}

async fn api_request(
    app: &axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder().method(method).uri(uri);

    if let Some(t) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    if body.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }

    let payload = match body {
        Some(b) => axum::body::Body::from(serde_json::to_vec(&b).unwrap()),
        None => axum::body::Body::empty(),
    };

    let req = builder.body(payload).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test]
async fn test_apply_move_turn_validation_and_legal_play() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool.clone());

    let (_token1, _h1, white_id) = create_test_player(&state, "w_turn").await;
    let (_token2, _h2, black_id) = create_test_player(&state, "b_turn").await;
    let (_token3, _h3, other_id) = create_test_player(&state, "other").await;

    let game_service = &state.game_service;
    let game = game_service
        .create_game(
            white_id,
            black_id,
            Pool::Blitz,
            TimeControl {
                initial_ms: 180_000,
                increment_ms: 2_000,
            },
            true,
        )
        .await
        .expect("game created");

    game_service
        .start_game(&game.public_id)
        .await
        .expect("game started");

    // 1. Wrong turn: Black tries to move first
    let res = game_service
        .apply_move(&game.public_id, black_id, "e7e5")
        .await;
    assert!(
        matches!(res, Err(GameServiceError::WrongTurn)),
        "black moving first must be WrongTurn"
    );

    // 2. Non-participant tries to move
    let res = game_service
        .apply_move(&game.public_id, other_id, "e2e4")
        .await;
    assert!(
        matches!(res, Err(GameServiceError::NotParticipant)),
        "non-participant move must be NotParticipant"
    );

    // 3. Illegal move
    let res = game_service
        .apply_move(&game.public_id, white_id, "e2e5")
        .await;
    assert!(
        matches!(res, Err(GameServiceError::IllegalMove(_))),
        "illegal move must be IllegalMove"
    );

    // 4. White plays legal move e4
    let move1 = game_service
        .apply_move(&game.public_id, white_id, "e2e4")
        .await
        .expect("e4 ok");
    assert_eq!(move1.move_data.ply, 1);
    assert_eq!(move1.move_data.uci, "e2e4");
    assert_eq!(move1.move_data.san, "e4");
    assert_eq!(move1.game.move_count, 1);
    assert_eq!(move1.game.status, "active");
    assert!(move1.completion.is_none());

    // 5. Rate limit: Black moves immediately (<40ms)
    let res = game_service
        .apply_move(&game.public_id, black_id, "e7e5")
        .await;
    assert!(
        matches!(res, Err(GameServiceError::RateLimited)),
        "move under 40ms must be RateLimited"
    );

    // Wait > 40ms to satisfy rate limit
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 6. Black plays legal move e5
    let move2 = game_service
        .apply_move(&game.public_id, black_id, "e7e5")
        .await
        .expect("e5 ok");
    assert_eq!(move2.move_data.ply, 2);
    assert_eq!(move2.move_data.uci, "e7e5");
    assert_eq!(move2.move_data.san, "e5");
    assert_eq!(move2.game.move_count, 2);

    // Verify moves in DB
    let db_moves = game_service
        .get_moves_by_game_id(game.id)
        .await
        .expect("get moves ok");
    assert_eq!(db_moves.len(), 2);
    assert_eq!(db_moves[0].uci, "e2e4");
    assert_eq!(db_moves[1].uci, "e7e5");
}

#[tokio::test]
async fn test_game_checkmate_completion_ratings_and_gold() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool.clone());

    let (_token1, _h1, white_id) = create_test_player(&state, "w_mate").await;
    let (_token2, _h2, black_id) = create_test_player(&state, "b_mate").await;

    let game_service = &state.game_service;
    let game = game_service
        .create_game(
            white_id,
            black_id,
            Pool::Blitz,
            TimeControl {
                initial_ms: 180_000,
                increment_ms: 2_000,
            },
            true,
        )
        .await
        .unwrap();

    game_service.start_game(&game.public_id).await.unwrap();

    // Fool's Mate: 1. f3 e5 2. g4 Qh4#
    let moves = [
        (white_id, "f2f3"),
        (black_id, "e7e5"),
        (white_id, "g2g4"),
        (black_id, "d8h4"),
    ];

    let mut last_res = None;
    for (player, uci) in moves {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let res = game_service
            .apply_move(&game.public_id, player, uci)
            .await
            .unwrap();
        last_res = Some(res);
    }

    let mate_res = last_res.unwrap();
    assert_eq!(mate_res.game.status, "finished");
    assert_eq!(mate_res.game.result.as_deref(), Some("black"));
    assert_eq!(mate_res.game.termination.as_deref(), Some("checkmate"));

    let completion = mate_res.completion.expect("completion data present");
    assert_eq!(completion.result, "black");
    assert_eq!(completion.termination, "checkmate");

    // Ratings applied
    let ratings = completion.ratings.expect("ratings applied for rated game");
    assert!(ratings.black.delta > 0.0, "winner gains rating");
    assert!(ratings.white.delta < 0.0, "loser loses rating");

    // Gold dispatched (25 for winner, 5 for loser)
    assert_eq!(completion.gold.black_earned, 25);
    assert_eq!(completion.gold.white_earned, 5);

    // Verify DB state
    let updated_game = game_service
        .get_game_by_public_id(&game.public_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated_game.status, "finished");
    assert_eq!(updated_game.result.as_deref(), Some("black"));
    assert_eq!(updated_game.termination.as_deref(), Some("checkmate"));
    assert!(updated_game.ended_at.is_some());
}

#[tokio::test]
async fn test_game_rest_endpoints_snapshot_moves_pgn() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool.clone());
    let app = router(state.clone());

    let (_token1, h1, white_id) = create_test_player(&state, "w_rest").await;
    let (_token2, h2, black_id) = create_test_player(&state, "b_rest").await;

    let game_service = &state.game_service;
    let game = game_service
        .create_game(
            white_id,
            black_id,
            Pool::Blitz,
            TimeControl {
                initial_ms: 180_000,
                increment_ms: 2_000,
            },
            true,
        )
        .await
        .unwrap();

    game_service.start_game(&game.public_id).await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;
    game_service
        .apply_move(&game.public_id, white_id, "e2e4")
        .await
        .unwrap();

    // 1. GET /v1/games/{id}
    let (status, snapshot) = api_request(
        &app,
        "GET",
        &format!("/v1/games/{}", game.public_id),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(snapshot["id"], game.public_id);
    assert_eq!(snapshot["white"]["handle"], h1);
    assert_eq!(snapshot["black"]["handle"], h2);
    assert_eq!(snapshot["ply"], 1);
    assert_eq!(snapshot["status"], "active");
    assert_eq!(snapshot["tc"]["initial_s"], 180);
    assert_eq!(snapshot["tc"]["increment_s"], 2);
    assert_eq!(snapshot["moves"].as_array().unwrap().len(), 1);

    // 2. GET /v1/games/{id}/moves
    let (status, moves_page) = api_request(
        &app,
        "GET",
        &format!("/v1/games/{}/moves?limit=10", game.public_id),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let items = moves_page["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["ply"], 1);
    assert_eq!(items[0]["uci"], "e2e4");
    assert_eq!(items[0]["san"], "e4");

    // 3. GET /v1/games/{id}/pgn
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/games/{}/pgn", game.public_id))
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/x-chess-pgn; charset=utf-8"
    );
    let pgn_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let pgn = String::from_utf8_lossy(&pgn_bytes);
    assert!(pgn.contains("[Site \"Chess Hustle\"]"));
    assert!(pgn.contains(&format!("[White \"{h1}\"]")));
    assert!(pgn.contains(&format!("[Black \"{h2}\"]")));
    assert!(pgn.contains("1. e4"));
}

#[tokio::test]
async fn test_game_resign_and_abort_rest_endpoints() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool.clone());
    let app = router(state.clone());

    let (token_w, _h1, white_id) = create_test_player(&state, "w_flow").await;
    let (token_b, _h2, black_id) = create_test_player(&state, "b_flow").await;
    let (token_other, _h3, _other_id) = create_test_player(&state, "other_flow").await;

    // --- Part A: Resign Flow ---
    let game_resign = state
        .game_service
        .create_game(
            white_id,
            black_id,
            Pool::Blitz,
            TimeControl {
                initial_ms: 180_000,
                increment_ms: 2_000,
            },
            true,
        )
        .await
        .unwrap();

    state
        .game_service
        .start_game(&game_resign.public_id)
        .await
        .unwrap();

    // 1. Non-participant cannot resign
    let (status, _body) = api_request(
        &app,
        "POST",
        &format!("/v1/games/{}/resign", game_resign.public_id),
        Some(&token_other),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // 2. White resigns
    let (status, body) = api_request(
        &app,
        "POST",
        &format!("/v1/games/{}/resign", game_resign.public_id),
        Some(&token_w),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "finished");
    assert_eq!(body["result"], "black");
    assert_eq!(body["termination"], "resignation");
    assert_eq!(body["gold"]["earned"], 5); // White lost, earned 5 gold

    // 3. Resign is idempotent
    let (status2, body2) = api_request(
        &app,
        "POST",
        &format!("/v1/games/{}/resign", game_resign.public_id),
        Some(&token_w),
        None,
    )
    .await;
    assert_eq!(status2, StatusCode::OK);
    assert_eq!(body2["status"], "finished");

    // --- Part B: Abort Flow ---
    let game_abort = state
        .game_service
        .create_game(
            white_id,
            black_id,
            Pool::Blitz,
            TimeControl {
                initial_ms: 180_000,
                increment_ms: 2_000,
            },
            true,
        )
        .await
        .unwrap();

    state
        .game_service
        .start_game(&game_abort.public_id)
        .await
        .unwrap();

    // Move count = 0 (<= 4), White aborts
    let (status, body) = api_request(
        &app,
        "POST",
        &format!("/v1/games/{}/abort", game_abort.public_id),
        Some(&token_b),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "aborted");
    assert_eq!(body["termination"], "aborted");

    // Second abort is idempotent
    let (status, body) = api_request(
        &app,
        "POST",
        &format!("/v1/games/{}/abort", game_abort.public_id),
        Some(&token_b),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "aborted");
}

#[tokio::test]
async fn test_realtime_websocket_game_move() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config.clone(), pool.clone());
    let app = router(state.clone());

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let (_token1, _h1, white_id) = create_test_player(&state, "ws_w").await;
    let (_token2, _h2, black_id) = create_test_player(&state, "ws_b").await;

    let game = state
        .game_service
        .create_game(
            white_id,
            black_id,
            Pool::Blitz,
            TimeControl {
                initial_ms: 180_000,
                increment_ms: 2_000,
            },
            true,
        )
        .await
        .unwrap();

    state
        .game_service
        .start_game(&game.public_id)
        .await
        .unwrap();

    // 1. Issue ticket for White
    let ticket = state.ticket_store.issue_ticket(white_id).await.unwrap();

    // 2. Open TCP stream to WebSocket server
    let mut stream = TcpStream::connect(local_addr).await.unwrap();

    let ws_key = "dGhlIHNhbXBsZSBub25jZQ==";
    let http_req = format!(
        "GET /ws?ticket={ticket} HTTP/1.1\r\n\
         Host: {local_addr}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {ws_key}\r\n\
         Sec-WebSocket-Version: 13\r\n\r\n"
    );

    stream.write_all(http_req.as_bytes()).await.unwrap();

    let mut resp_buf = [0u8; 1024];
    let n = stream.read(&mut resp_buf).await.unwrap();
    let resp_str = String::from_utf8_lossy(&resp_buf[..n]);
    assert!(resp_str.starts_with("HTTP/1.1 101 Switching Protocols"));

    let mask = [0xaa, 0xbb, 0xcc, 0xdd];

    // 3. Hello
    let hello = ClientCommand::Hello {
        client: Some("test_ws@1.0".into()),
        last_ids: None,
    };
    write_client_frame(
        &mut stream,
        &Frame::Text(serde_json::to_string(&hello).unwrap()),
        mask,
    )
    .await
    .unwrap();

    let f1 = read_frame(&mut stream).await.unwrap().expect("hello.ack");
    match f1 {
        Frame::Text(txt) => {
            let env: Envelope = serde_json::from_str(&txt).unwrap();
            assert_eq!(env.event_type, "hello.ack");
        }
        _ => panic!("expected hello.ack"),
    }

    // 4. Subscribe to g:{game_id}
    let sub = ClientCommand::Subscribe {
        ch: format!("g:{}", game.public_id),
    };
    write_client_frame(
        &mut stream,
        &Frame::Text(serde_json::to_string(&sub).unwrap()),
        mask,
    )
    .await
    .unwrap();

    let f2 = read_frame(&mut stream)
        .await
        .unwrap()
        .expect("subscribe.ack");
    match f2 {
        Frame::Text(txt) => {
            let env: Envelope = serde_json::from_str(&txt).unwrap();
            assert_eq!(env.event_type, "subscribe.ack");
        }
        _ => panic!("expected subscribe.ack"),
    }

    // 5. Send ClientCommand::GameMove
    let mv_cmd = ClientCommand::GameMove {
        game_id: game.public_id.clone(),
        uci: "e2e4".to_string(),
    };
    write_client_frame(
        &mut stream,
        &Frame::Text(serde_json::to_string(&mv_cmd).unwrap()),
        mask,
    )
    .await
    .unwrap();

    // 6. Receive game.move event published to ws:g:{game_id}
    let f3 = read_frame(&mut stream).await.unwrap().expect("game.move");
    match f3 {
        Frame::Text(txt) => {
            let env: Envelope = serde_json::from_str(&txt).unwrap();
            assert_eq!(env.event_type, "game.move");
            assert_eq!(env.ch, format!("g:{}", game.public_id));
            assert_eq!(env.data["ply"], 1);
            assert_eq!(env.data["uci"], "e2e4");
            assert_eq!(env.data["san"], "e4");
        }
        _ => panic!("expected game.move text frame"),
    }

    // 7. Verify move in DB
    let db_moves = state
        .game_service
        .get_moves_by_game_id(game.id)
        .await
        .unwrap();
    assert_eq!(db_moves.len(), 1);
    assert_eq!(db_moves[0].uci, "e2e4");
}

#[tokio::test]
async fn test_draw_offer_decline_and_agreement_flow() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skipping test_draw_offer_decline_and_agreement_flow: no DB");
        return;
    };

    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool.clone());
    let app = router(state.clone());

    let (token_w, _, user_w) = create_test_player(&state, "draw_w").await;
    let (token_b, _, user_b) = create_test_player(&state, "draw_b").await;

    let game = state
        .game_service
        .create_game(
            user_w,
            user_b,
            Pool::Blitz,
            TimeControl {
                initial_ms: 180_000,
                increment_ms: 2_000,
            },
            true,
        )
        .await
        .expect("game created");

    let _ = state
        .game_service
        .start_game(&game.public_id)
        .await
        .unwrap();

    // White plays e4
    state
        .game_service
        .apply_move(&game.public_id, user_w, "e2e4")
        .await
        .unwrap();

    // 1. Black offers draw
    let (status, body) = api_request(
        &app,
        "POST",
        &format!("/v1/games/{}/draw-offer", game.public_id),
        Some(&token_b),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["draw_offer"], "black");

    // 2. Black tries to accept own draw offer -> 403 Forbidden
    let (status, body) = api_request(
        &app,
        "POST",
        &format!("/v1/games/{}/draw-accept", game.public_id),
        Some(&token_b),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "draw.own_offer");

    // 3. White declines draw
    let (status, body) = api_request(
        &app,
        "POST",
        &format!("/v1/games/{}/draw-decline", game.public_id),
        Some(&token_w),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["draw_offer"].is_null());

    // 4. White offers draw
    let (status, body) = api_request(
        &app,
        "POST",
        &format!("/v1/games/{}/draw-offer", game.public_id),
        Some(&token_w),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["draw_offer"], "white");

    // 5. Black accepts draw -> game finishes by agreement
    let (status, body) = api_request(
        &app,
        "POST",
        &format!("/v1/games/{}/draw-accept", game.public_id),
        Some(&token_b),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "finished");
    assert_eq!(body["result"], "draw");
    assert_eq!(body["termination"], "agreement");
    assert_eq!(body["gold"]["earned"], 10);

    // Verify DB row
    let db_game = state
        .game_service
        .get_game_by_public_id(&game.public_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(db_game.status, "finished");
    assert_eq!(db_game.result.as_deref(), Some("draw"));
    assert_eq!(db_game.termination.as_deref(), Some("agreement"));
}

#[tokio::test]
async fn test_claim_timeout_endpoint_real_flow() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skipping test_claim_timeout_endpoint_real_flow: no DB");
        return;
    };

    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool.clone());
    let app = router(state.clone());

    let (token_w, _, user_w) = create_test_player(&state, "to_w").await;
    let (token_b, _, user_b) = create_test_player(&state, "to_b").await;

    let game = state
        .game_service
        .create_game(
            user_w,
            user_b,
            Pool::Bullet,
            TimeControl {
                initial_ms: 60_000,
                increment_ms: 0,
            },
            true,
        )
        .await
        .expect("game created");

    let _ = state
        .game_service
        .start_game(&game.public_id)
        .await
        .unwrap();

    // White moves e4 -> Black's turn to move
    state
        .game_service
        .apply_move(&game.public_id, user_w, "e2e4")
        .await
        .unwrap();

    // 1. Black tries to claim timeout on Black's own turn -> 409 clock.not_opponent_turn
    let (status, body) = api_request(
        &app,
        "POST",
        &format!("/v1/games/{}/claim-timeout", game.public_id),
        Some(&token_b),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "clock.not_opponent_turn");

    // 2. White tries to claim timeout immediately -> 409 clock.not_expired (clock hasn't run out)
    let (status, body) = api_request(
        &app,
        "POST",
        &format!("/v1/games/{}/claim-timeout", game.public_id),
        Some(&token_w),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "clock.not_expired");

    // 3. Fast-forward clock in DB: last tick 2 hours ago
    sqlx::query(
        "UPDATE games SET last_tick_ts = now() - INTERVAL '2 hours', b_remaining_ms = 500 WHERE id = $1",
    )
    .bind(game.id)
    .execute(&pool)
    .await
    .unwrap();

    // 4. White claims timeout -> succeeds, White wins on timeout!
    let (status, body) = api_request(
        &app,
        "POST",
        &format!("/v1/games/{}/claim-timeout", game.public_id),
        Some(&token_w),
        None,
    )
    .await;
    eprintln!("claim-timeout response: status={status:?}, body={body:#?}");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "finished");
    assert_eq!(body["result"], "white");
    assert_eq!(body["termination"], "timeout");
    assert_eq!(body["gold"]["earned"], 25);
    assert!(body["ratings"]["white"]["delta"].as_f64().unwrap() > 0.0);
    assert!(body["ratings"]["black"]["delta"].as_f64().unwrap() < 0.0);

    // Verify DB state
    let db_game = state
        .game_service
        .get_game_by_public_id(&game.public_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(db_game.status, "finished");
    assert_eq!(db_game.result.as_deref(), Some("white"));
    assert_eq!(db_game.termination.as_deref(), Some("timeout"));
}
