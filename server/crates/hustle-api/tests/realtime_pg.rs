use axum::http::{header, Request, StatusCode};
use hustle_api::{router, AppState};
use hustle_chess::GameService;
use hustle_domain::{Pool, TimeControl};
use hustle_identity::signup::{Consents, ProfileInput, VerifiedIdentity};
use hustle_kernel::ServerConfig;
use hustle_realtime::{read_frame, write_client_frame, ClientCommand, Envelope, Frame};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::net::TcpListener;
use tower::ServiceExt;

fn counter() -> u64 {
    static N: AtomicU64 = AtomicU64::new(100);
    N.fetch_add(1, Ordering::Relaxed)
}

fn phone() -> String {
    format!("+9197{:010}", counter())
}

fn handle() -> String {
    format!("rt_user_{}", counter())
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

#[tokio::test]
async fn realtime_ticket_and_ws_upgrade_flow() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config.clone(), pool.clone());
    let app = router(state.clone());

    // 1. Create a real user in Postgres
    let user_phone = phone();
    let user_handle = handle();
    let res = state
        .signup
        .signup(
            VerifiedIdentity::Phone(user_phone),
            consents(),
            ProfileInput {
                handle: user_handle,
                display_name: "WS Player".to_string(),
            },
            Some("device_rt_1"),
            Some("127.0.0.1"),
        )
        .await
        .expect("signup ok");

    let access_token = res.access_token;
    let user_id = res.user_id;

    // 2. Unauthenticated request to /v1/realtime/ticket is 401
    let unauth_req = Request::builder()
        .uri("/v1/realtime/ticket")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let unauth_res = app.clone().oneshot(unauth_req).await.unwrap();
    assert_eq!(unauth_res.status(), StatusCode::UNAUTHORIZED);

    // 3. Authenticated request issues valid ticket
    let auth_req = Request::builder()
        .uri("/v1/realtime/ticket")
        .method("GET")
        .header(header::AUTHORIZATION, format!("Bearer {access_token}"))
        .body(axum::body::Body::empty())
        .unwrap();

    let auth_res = app.clone().oneshot(auth_req).await.unwrap();
    assert_eq!(auth_res.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(auth_res.into_body(), usize::MAX)
        .await
        .unwrap();
    let ticket_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let ticket = ticket_json["ticket"].as_str().expect("ticket string");
    assert_eq!(ticket_json["ws_url"], "/ws");
    assert_eq!(ticket_json["expires_in"], 30);

    // 4. Test atomic consumption: consuming the ticket works the first time, fails the second
    let consumed_uid = state
        .ticket_store
        .consume_ticket(ticket)
        .await
        .expect("consume ok");
    assert_eq!(consumed_uid, Some(user_id));

    let reused_uid = state
        .ticket_store
        .consume_ticket(ticket)
        .await
        .expect("consume ok");
    assert_eq!(reused_uid, None, "ticket cannot be consumed twice");
}

#[tokio::test]
async fn ws_connection_full_session_flow_tcp() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config.clone(), pool.clone());
    let app = router(state.clone());

    // Bind to local ephemeral port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Create 2 users: ananya and vikram
    let u1 = state
        .signup
        .signup(
            VerifiedIdentity::Phone(phone()),
            consents(),
            ProfileInput {
                handle: handle(),
                display_name: "Ananya".to_string(),
            },
            Some("device_ananya"),
            Some("127.0.0.1"),
        )
        .await
        .unwrap();

    let u2 = state
        .signup
        .signup(
            VerifiedIdentity::Phone(phone()),
            consents(),
            ProfileInput {
                handle: handle(),
                display_name: "Vikram".to_string(),
            },
            Some("device_vikram"),
            Some("127.0.0.1"),
        )
        .await
        .unwrap();

    // Create a real game between ananya (white) and vikram (black) in Postgres
    let game_service = GameService::new(pool.clone());
    let game = game_service
        .create_game(
            u1.user_id,
            u2.user_id,
            Pool::Blitz,
            TimeControl {
                initial_ms: 180_000,
                increment_ms: 2_000,
            },
            true,
        )
        .await
        .unwrap();

    // 1. Issue a WebSocket ticket for Ananya
    let ticket = state.ticket_store.issue_ticket(u1.user_id).await.unwrap();

    // 2. Open TCP stream to the server
    use tokio::net::TcpStream;
    let mut stream = TcpStream::connect(local_addr).await.unwrap();

    // 3. Send HTTP 101 WebSocket Upgrade request
    let ws_key = "dGhlIHNhbXBsZSBub25jZQ==";
    let http_req = format!(
        "GET /ws?ticket={ticket} HTTP/1.1\r\n\
         Host: {local_addr}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {ws_key}\r\n\
         Sec-WebSocket-Version: 13\r\n\r\n"
    );

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    stream.write_all(http_req.as_bytes()).await.unwrap();

    // Read HTTP 101 response
    let mut resp_buf = [0u8; 1024];
    let n = stream.read(&mut resp_buf).await.unwrap();
    let resp_str = String::from_utf8_lossy(&resp_buf[..n]);
    assert!(
        resp_str.starts_with("HTTP/1.1 101 Switching Protocols"),
        "expected 101 Switching Protocols, got: {resp_str}"
    );

    let mask = [0xde, 0xad, 0xbe, 0xef];

    // 4. Send hello
    let hello = ClientCommand::Hello {
        client: Some("web@1.0.0".into()),
        last_ids: None,
    };
    write_client_frame(
        &mut stream,
        &Frame::Text(serde_json::to_string(&hello).unwrap()),
        mask,
    )
    .await
    .unwrap();

    // 5. Receive hello.ack
    let f1 = read_frame(&mut stream)
        .await
        .unwrap()
        .expect("f1 hello.ack");
    match f1 {
        Frame::Text(txt) => {
            let env: Envelope = serde_json::from_str(&txt).unwrap();
            assert_eq!(env.event_type, "hello.ack");
            assert_eq!(env.data["user_id"], u1.user_id);
        }
        _ => panic!("expected hello.ack text frame"),
    }

    // 6. Send ping
    write_client_frame(
        &mut stream,
        &Frame::Text(serde_json::to_string(&ClientCommand::Ping).unwrap()),
        mask,
    )
    .await
    .unwrap();

    let f2 = read_frame(&mut stream).await.unwrap().expect("f2 pong");
    match f2 {
        Frame::Text(txt) => {
            let v: serde_json::Value = serde_json::from_str(&txt).unwrap();
            assert_eq!(v["type"], "pong");
        }
        _ => panic!("expected pong text frame"),
    }

    // 7. Subscribe to the real game as Ananya (authorized player in game)
    let sub_game = ClientCommand::Subscribe {
        ch: format!("g:{}", game.public_id),
    };
    write_client_frame(
        &mut stream,
        &Frame::Text(serde_json::to_string(&sub_game).unwrap()),
        mask,
    )
    .await
    .unwrap();

    let f3 = read_frame(&mut stream)
        .await
        .unwrap()
        .expect("f3 subscribe.ack");
    match f3 {
        Frame::Text(txt) => {
            let env: Envelope = serde_json::from_str(&txt).unwrap();
            assert_eq!(env.event_type, "subscribe.ack");
            assert_eq!(env.data["ch"], format!("g:{}", game.public_id));
        }
        _ => panic!("expected subscribe.ack text frame"),
    }

    // 8. Subscribe to an unauthorized private game
    let sub_unauthorized = ClientCommand::Subscribe {
        ch: "g:01JDNONEXISTENTORPRIVATEGAME".to_string(),
    };
    write_client_frame(
        &mut stream,
        &Frame::Text(serde_json::to_string(&sub_unauthorized).unwrap()),
        mask,
    )
    .await
    .unwrap();

    let f4 = read_frame(&mut stream).await.unwrap().expect("f4 ws.error");
    match f4 {
        Frame::Text(txt) => {
            let env: Envelope = serde_json::from_str(&txt).unwrap();
            assert_eq!(env.event_type, "ws.error");
            assert_eq!(env.data["code"], "auth.forbidden");
        }
        _ => panic!("expected ws.error text frame"),
    }

    // 9. Clean Close
    write_client_frame(
        &mut stream,
        &Frame::Close(Some((1000, "done".into()))),
        mask,
    )
    .await
    .unwrap();
}
