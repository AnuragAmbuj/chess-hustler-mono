use hustle_realtime::{
    read_frame, write_client_frame, AllowUserAndCallback, ClientCommand, Envelope, EventPublisher,
    Frame, TicketStore, WsSession,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::duplex;

fn test_redis_client() -> redis::Client {
    let url = std::env::var("CH_TEST_REDIS_URL")
        .or_else(|_| std::env::var("CH_REDIS_URL"))
        .unwrap_or_else(|_| {
            if std::path::Path::new("/tmp/opencode/redis.sock").exists() {
                "redis+unix:///tmp/opencode/redis.sock".to_string()
            } else {
                "redis://127.0.0.1:6379".to_string()
            }
        });
    redis::Client::open(url).expect("valid redis connection")
}

#[tokio::test]
async fn ticket_store_issue_and_atomic_single_use_consumption() {
    let client = test_redis_client();
    let store = TicketStore::new(client.clone());

    let user_id = 98765i64;
    let ticket = store.issue_ticket(user_id).await.expect("issue ticket ok");
    assert!(!ticket.is_empty(), "ticket must be a valid non-empty ULID");

    // First consumption MUST succeed
    let consumed = store.consume_ticket(&ticket).await.expect("consume ok");
    assert_eq!(consumed, Some(user_id));

    // Second consumption MUST fail (single-use token enforcement)
    let replayed = store.consume_ticket(&ticket).await.expect("consume ok");
    assert_eq!(replayed, None, "reused ticket must return None");

    // Random non-existent ticket must fail
    let nonexistent = store
        .consume_ticket("01JDNONEXISTENTTICKET000000")
        .await
        .expect("consume ok");
    assert_eq!(nonexistent, None);
}

#[tokio::test]
async fn event_publisher_streams_and_gap_replay() {
    let client = test_redis_client();
    let publisher = EventPublisher::new(client.clone());

    let ch = format!("u:test_{}", ulid::Ulid::new());

    // Publish three sequential events
    let ev1 = publisher
        .publish(
            &ch,
            "economy.update",
            serde_json::json!({"delta": 500, "balance": 50500}),
        )
        .await
        .expect("publish 1 ok");

    let ev2 = publisher
        .publish(
            &ch,
            "matchmaking.matched",
            serde_json::json!({"game_id": "01JD_TEST_GAME", "color": "white"}),
        )
        .await
        .expect("publish 2 ok");

    let ev3 = publisher
        .publish(
            &ch,
            "daily.available",
            serde_json::json!({"streak": 3, "reward": 50}),
        )
        .await
        .expect("publish 3 ok");

    assert!(!ev1.id.is_empty());
    assert!(!ev2.id.is_empty());
    assert!(!ev3.id.is_empty());

    // Replay gap starting from after ev1.id
    let gap = publisher
        .read_gap(&ch, &ev1.id, 10)
        .await
        .expect("read gap ok");
    assert_eq!(gap.len(), 2, "must return exactly ev2 and ev3");
    assert_eq!(gap[0].event_type, "matchmaking.matched");
    assert_eq!(gap[1].event_type, "daily.available");
    assert_eq!(gap[0].id, ev2.id);
    assert_eq!(gap[1].id, ev3.id);
}

#[tokio::test]
async fn ws_session_full_lifecycle_pg_redis_e2e() {
    let client = test_redis_client();
    let publisher = EventPublisher::new(client.clone());

    let user_id = 777123i64;
    let u_ch = format!("u:{user_id}");

    // Pre-populate 2 events into user's stream for gap replay verification
    let ev1 = publisher
        .publish(
            &u_ch,
            "init.welcome",
            serde_json::json!({"note": "welcome"}),
        )
        .await
        .unwrap();

    let ev2 = publisher
        .publish(&u_ch, "economy.update", serde_json::json!({"delta": 100}))
        .await
        .unwrap();

    // Authorizer allowing user channel and game "g:game_allowed", but denying "g:game_forbidden"
    let authorizer = Arc::new(AllowUserAndCallback::new(
        move |_uid: i64, ch: String| async move { ch == "g:game_allowed" },
    ));

    let session = Arc::new(WsSession::new(
        user_id,
        publisher.clone(),
        authorizer,
        client.clone(),
    ));

    let (mut client_stream, server_stream) = duplex(8192);

    // Spawn server session task
    tokio::spawn(async move {
        session.run(server_stream).await;
    });

    let mask = [0x11, 0x22, 0x33, 0x44];

    // 1. Client sends hello with resume cursor pointing to ev1.id
    let mut last_ids = HashMap::new();
    last_ids.insert(u_ch.clone(), ev1.id.clone());

    let hello_cmd = ClientCommand::Hello {
        client: Some("web@1.0.0".to_string()),
        last_ids: Some(last_ids),
    };
    let hello_frame = Frame::Text(serde_json::to_string(&hello_cmd).unwrap());
    write_client_frame(&mut client_stream, &hello_frame, mask)
        .await
        .unwrap();

    // 2. Expect hello.ack
    let f1 = read_frame(&mut client_stream)
        .await
        .unwrap()
        .expect("frame 1");
    match f1 {
        Frame::Text(txt) => {
            let env: Envelope = serde_json::from_str(&txt).unwrap();
            assert_eq!(env.event_type, "hello.ack");
            assert_eq!(env.data["user_id"], user_id);
        }
        _ => panic!("expected text frame for hello.ack"),
    }

    // 3. Expect gap event ev2 replayed
    let f2 = read_frame(&mut client_stream)
        .await
        .unwrap()
        .expect("frame 2");
    match f2 {
        Frame::Text(txt) => {
            let env: Envelope = serde_json::from_str(&txt).unwrap();
            assert_eq!(env.event_type, "economy.update");
            assert_eq!(env.id, ev2.id);
        }
        _ => panic!("expected text frame for replayed event"),
    }

    // 4. Expect resume.ack
    let f3 = read_frame(&mut client_stream)
        .await
        .unwrap()
        .expect("frame 3");
    match f3 {
        Frame::Text(txt) => {
            let env: Envelope = serde_json::from_str(&txt).unwrap();
            assert_eq!(env.event_type, "resume.ack");
            assert_eq!(env.data["per_channel"][&u_ch]["replayed"], 1);
        }
        _ => panic!("expected text frame for resume.ack"),
    }

    // 5. Client sends ping
    let ping_cmd = ClientCommand::Ping;
    let ping_frame = Frame::Text(serde_json::to_string(&ping_cmd).unwrap());
    write_client_frame(&mut client_stream, &ping_frame, mask)
        .await
        .unwrap();

    let f4 = read_frame(&mut client_stream)
        .await
        .unwrap()
        .expect("frame 4");
    match f4 {
        Frame::Text(txt) => {
            let v: serde_json::Value = serde_json::from_str(&txt).unwrap();
            assert_eq!(v["type"], "pong");
        }
        _ => panic!("expected pong text frame"),
    }

    // Give Redis pubsub a short moment to establish pattern listeners
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // 6. Test Live Event Fanout: Publisher pushes an event to u_ch
    publisher
        .publish(
            &u_ch,
            "friend.presence",
            serde_json::json!({"user_id": 999, "online": true}),
        )
        .await
        .unwrap();

    let f5 = read_frame(&mut client_stream)
        .await
        .unwrap()
        .expect("frame 5");
    match f5 {
        Frame::Text(txt) => {
            let env: Envelope = serde_json::from_str(&txt).unwrap();
            assert_eq!(env.event_type, "friend.presence");
            assert_eq!(env.data["online"], true);
        }
        _ => panic!("expected live friend.presence event"),
    }

    // 7. Subscribe to authorized game channel
    let sub_cmd = ClientCommand::Subscribe {
        ch: "g:game_allowed".to_string(),
    };
    let sub_frame = Frame::Text(serde_json::to_string(&sub_cmd).unwrap());
    write_client_frame(&mut client_stream, &sub_frame, mask)
        .await
        .unwrap();

    let f6 = read_frame(&mut client_stream)
        .await
        .unwrap()
        .expect("frame 6");
    match f6 {
        Frame::Text(txt) => {
            let env: Envelope = serde_json::from_str(&txt).unwrap();
            assert_eq!(env.event_type, "subscribe.ack");
            assert_eq!(env.data["ch"], "g:game_allowed");
        }
        _ => panic!("expected subscribe.ack"),
    }

    // 8. Subscribe to forbidden game channel
    let sub_bad_cmd = ClientCommand::Subscribe {
        ch: "g:game_forbidden".to_string(),
    };
    let sub_bad_frame = Frame::Text(serde_json::to_string(&sub_bad_cmd).unwrap());
    write_client_frame(&mut client_stream, &sub_bad_frame, mask)
        .await
        .unwrap();

    let f7 = read_frame(&mut client_stream)
        .await
        .unwrap()
        .expect("frame 7");
    match f7 {
        Frame::Text(txt) => {
            let env: Envelope = serde_json::from_str(&txt).unwrap();
            assert_eq!(env.event_type, "ws.error");
            assert_eq!(env.data["code"], "auth.forbidden");
        }
        _ => panic!("expected ws.error auth.forbidden"),
    }

    // 9. Clean Close
    let close_frame = Frame::Close(Some((1000, "client exit".to_string())));
    write_client_frame(&mut client_stream, &close_frame, mask)
        .await
        .unwrap();
}
