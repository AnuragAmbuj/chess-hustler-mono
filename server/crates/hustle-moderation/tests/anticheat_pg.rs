use hustle_chess::GameService;
use hustle_domain::{Pool, TimeControl};
use hustle_identity::signup::{Consents, ProfileInput, SignupService, VerifiedIdentity};
use hustle_identity::TokenService;
use hustle_moderation::{CreateSanctionRequest, ModerationService, ResolveFlagRequest};
use hustle_realtime::EventPublisher;
use std::sync::atomic::{AtomicU64, Ordering};
use time::OffsetDateTime;

fn counter() -> u64 {
    static N: AtomicU64 = AtomicU64::new(40_000);
    N.fetch_add(1, Ordering::Relaxed)
        + (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            % 10_000_000)
}

fn phone() -> String {
    format!("+9197{:010}", counter())
}

fn handle() -> String {
    format!("ac_usr_{}", counter())
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

async fn make_user(pool: &sqlx::PgPool) -> (i64, String, String) {
    let token_service = TokenService::new(b"test_secret_key_32_bytes_long_ok!");
    let signup = SignupService::new(pool.clone(), token_service, 50_000);
    let p = phone();
    let h = handle();
    let outcome = signup
        .signup(
            VerifiedIdentity::Phone(p),
            consents(),
            ProfileInput {
                handle: h.clone(),
                display_name: format!("Player {h}"),
            },
            None,
            None,
        )
        .await
        .expect("signup failed");

    let public_id: String = sqlx::query_scalar("SELECT public_id FROM users WHERE id = $1")
        .bind(outcome.user_id)
        .fetch_one(pool)
        .await
        .unwrap();

    (outcome.user_id, public_id, h)
}

#[tokio::test]
async fn test_anticheat_move_time_regularity_detection() {
    let Some(pool) = hustle_testkit::pool().await else {
        return;
    };
    let redis_client =
        redis::Client::open("redis+unix:///tmp/opencode/redis.sock").expect("valid redis url");
    let publisher = EventPublisher::new(redis_client.clone());
    let mod_svc = ModerationService::new(pool.clone(), redis_client, publisher);
    let game_svc = GameService::new(pool.clone());

    let (u1, _, _) = make_user(&pool).await;
    let (u2, _, _) = make_user(&pool).await;

    // Create and start game
    let tc = TimeControl {
        initial_ms: 300_000,
        increment_ms: 0,
    };
    let game = game_svc
        .create_game(u1, u2, Pool::Blitz, tc, true)
        .await
        .unwrap();
    game_svc.start_game(&game.public_id).await.unwrap();

    // Insert 18 moves where player 2 (black) plays moves with strictly identical 1000ms intervals (std dev = 0.0)
    let start_ts = OffsetDateTime::now_utc();
    let moves_seq = [
        ("e2e4", "e4"),
        ("e7e5", "e5"),
        ("g1f3", "Nf3"),
        ("b8c6", "Nc6"),
        ("f1b5", "Bb5"),
        ("a7a6", "a6"),
        ("b5a4", "Ba4"),
        ("g8f6", "Nf6"),
        ("e1g1", "O-O"),
        ("f8e7", "Be7"),
        ("f1e1", "Re1"),
        ("b7b5", "b5"),
        ("a4b3", "Bb3"),
        ("d7d6", "d6"),
        ("c2c3", "c3"),
        ("e8g8", "O-O"),
        ("h2h3", "h3"),
        ("c6b8", "Nb8"),
    ];

    let white_delays = [2000i64, 5500, 1200, 9300, 3100, 14200, 6700, 21000, 8400];
    let mut white_cum = 0i64;
    let mut black_cum = 0i64;

    for (i, (uci, san)) in moves_seq.iter().enumerate() {
        let ply = (i + 1) as i32;
        let move_ts = if ply % 2 == 1 {
            let idx = (ply as usize - 1) / 2;
            white_cum += white_delays[idx];
            start_ts + time::Duration::milliseconds(white_cum)
        } else {
            black_cum += 1000;
            start_ts + time::Duration::milliseconds(black_cum)
        };

        sqlx::query(
            r#"
            INSERT INTO moves (game_id, ply, uci, san, fen_after, w_remaining_ms, b_remaining_ms, ts)
            VALUES ($1, $2, $3, $4, 'fen_placeholder', 290000, 290000, $5)
            "#,
        )
        .bind(game.id)
        .bind(ply)
        .bind(uci)
        .bind(san)
        .bind(move_ts)
        .execute(&pool)
        .await
        .unwrap();
    }

    // Mark game finished
    sqlx::query(
        "UPDATE games SET status = 'finished', result = 'white', termination = 'resignation', move_count = 18 WHERE id = $1",
    )
    .bind(game.id)
    .execute(&pool)
    .await
    .unwrap();

    // Run offline anti-cheat analysis
    let flags = mod_svc.run_anti_cheat_analysis(game.id).await.unwrap();

    // Verify move_time flag was emitted for Black (u2)
    let move_time_flag = flags.iter().find(|f| f.kind == "move_time");
    assert!(
        move_time_flag.is_some(),
        "Expected move_time anomaly flag to be generated"
    );
    let f = move_time_flag.unwrap();
    assert_eq!(f.subject_user_id, u2);
    assert_eq!(f.status, "open");
    assert!(
        f.score >= 0.75,
        "Expected high confidence score for uniform intervals"
    );
    assert_eq!(f.evidence["color"], "black");

    // Idempotency: re-running analysis should not duplicate flag
    let flags_rerun = mod_svc.run_anti_cheat_analysis(game.id).await.unwrap();
    assert!(
        flags_rerun.iter().all(|x| x.kind != "move_time"),
        "Duplicate flag should not be inserted on rerun"
    );
}

#[tokio::test]
async fn test_anticheat_pair_farming_and_admin_review_lifecycle() {
    let Some(pool) = hustle_testkit::pool().await else {
        return;
    };
    let redis_client =
        redis::Client::open("redis+unix:///tmp/opencode/redis.sock").expect("valid redis url");
    let publisher = EventPublisher::new(redis_client.clone());
    let mod_svc = ModerationService::new(pool.clone(), redis_client, publisher);
    let game_svc = GameService::new(pool.clone());

    let (u_winner, _, h_winner) = make_user(&pool).await;
    let (u_loser, _, _) = make_user(&pool).await;

    let tc = TimeControl {
        initial_ms: 180_000,
        increment_ms: 2000,
    };

    // Seed 3 games where u_loser quickly resigns in <= 4 moves to u_winner
    let mut last_game_id = 0i64;
    for _ in 0..3 {
        let g = game_svc
            .create_game(u_winner, u_loser, Pool::Blitz, tc, true)
            .await
            .unwrap();
        last_game_id = g.id;
        sqlx::query(
            r#"
            UPDATE games
            SET status = 'finished', result = 'white', termination = 'resignation', move_count = 3
            WHERE id = $1
            "#,
        )
        .bind(g.id)
        .execute(&pool)
        .await
        .unwrap();
    }

    // Run analyzer on the latest game
    let flags = mod_svc.run_anti_cheat_analysis(last_game_id).await.unwrap();
    let pair_flag = flags.iter().find(|f| f.kind == "pair_farming");
    assert!(
        pair_flag.is_some(),
        "Expected pair_farming flag after 3 quick resignations"
    );
    let f = pair_flag.unwrap();
    assert_eq!(f.subject_user_id, u_winner);
    assert_eq!(f.related_user_id, Some(u_loser));
    assert_eq!(f.status, "open");
    assert!(f.score >= 0.85);

    // Test Admin listing of flags
    let open_flags = mod_svc.list_flags(Some("open"), None, 20, 0).await.unwrap();
    assert!(
        open_flags.iter().any(|x| x.id == f.id),
        "Flag should appear in open flags queue"
    );

    // Test Moderator resolution of the flag
    let resolve_res = mod_svc
        .resolve_flag(
            f.id,
            "mod_lead_actor",
            &ResolveFlagRequest {
                resolution: "confirmed".into(),
                notes: Some("Confirmed collusive pair farming win trading".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(resolve_res.status, "confirmed");
    assert_eq!(resolve_res.reviewed_by.as_deref(), Some("mod_lead_actor"));

    // Test applying Sanction ladder (formal warning -> mute -> matchmaking suspension)
    let sanction = mod_svc
        .create_sanction(
            u_winner,
            "admin_system",
            &CreateSanctionRequest {
                kind: "mm_suspension".into(),
                reason: "Collusive pair farming / win trading violation".into(),
                expires_at: Some(
                    (OffsetDateTime::now_utc() + time::Duration::days(7))
                        .format(&time::format_description::well_known::Rfc3339)
                        .unwrap(),
                ),
                evidence: Some(f.evidence.clone()),
            },
        )
        .await
        .unwrap();
    assert_eq!(sanction.user_id, u_winner);
    assert_eq!(sanction.kind, "mm_suspension");
    assert_eq!(sanction.issued_by, "admin_system");

    // Verify sanctions list
    let sanctions_list = mod_svc.list_sanctions(u_winner).await.unwrap();
    assert_eq!(sanctions_list.len(), 1);
    assert_eq!(sanctions_list[0].id, sanction.id);

    // Verify admin player inspection
    let admin_player = mod_svc.get_player_admin_detail(&h_winner).await.unwrap();
    assert_eq!(admin_player.id, u_winner);
    assert_eq!(admin_player.handle, h_winner);
    assert_eq!(admin_player.active_sanctions.len(), 1);

    // Verify audit log entries were recorded
    let audit_logs = mod_svc.get_audit_logs(10, 0).await.unwrap();
    assert!(
        audit_logs
            .iter()
            .any(|a| a.action == "resolve_flag" && a.actor == "mod_lead_actor"),
        "Audit log must contain resolve_flag action"
    );
    assert!(
        audit_logs
            .iter()
            .any(|a| a.action == "create_sanction" && a.actor == "admin_system"),
        "Audit log must contain create_sanction action"
    );
}
