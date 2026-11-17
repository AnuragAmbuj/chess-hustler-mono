//! L3 integration tests — real Postgres via hustle-testkit (skips when unset).

use hustle_gold::{GoldLedger, GoldReason, GrantResult, LedgerError, LedgerOp, PgGoldLedger};

fn op(user: i64, delta: i64, reason: GoldReason, idem: Option<&str>) -> LedgerOp {
    LedgerOp {
        user_id: user,
        delta,
        reason,
        ref_type: None,
        ref_id: None,
        idem_key: idem.map(str::to_string),
        meta: serde_json::json!({}),
    }
}

#[tokio::test]
async fn welcome_grant_applies_and_balance_reflects() {
    let Some(pg) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let user = hustle_testkit::make_user(&pg, "gold").await;
    let ledger = PgGoldLedger::new(pg.clone());

    let out = ledger
        .grant(op(user, 50_000, GoldReason::Welcome, Some("welcome:t1")))
        .await
        .unwrap();
    let GrantResult::Applied {
        ledger_id,
        balance_after,
    } = out
    else {
        panic!("first grant must apply, got {out:?}");
    };
    assert!(ledger_id > 0);
    assert_eq!(balance_after, 50_000);
    assert_eq!(ledger.balance(user).await.unwrap(), 50_000);
}

#[tokio::test]
async fn idem_replay_returns_original_balance_without_second_row() {
    let Some(pg) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let user = hustle_testkit::make_user(&pg, "gold").await;
    let ledger = PgGoldLedger::new(pg.clone());

    let first = ledger
        .grant(op(user, 500, GoldReason::DailyStreak, Some("daily:t2")))
        .await
        .unwrap();
    let second = ledger
        .grant(op(user, 500, GoldReason::DailyStreak, Some("daily:t2")))
        .await
        .unwrap();

    assert!(matches!(
        first,
        GrantResult::Applied {
            balance_after: 500,
            ..
        }
    ));
    assert_eq!(second, GrantResult::Duplicate { balance_after: 500 });
    assert_eq!(ledger.balance(user).await.unwrap(), 500);

    // Exactly one ledger row for that idem key.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM gold_ledger WHERE user_id = $1")
        .bind(user)
        .fetch_one(&pg)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn overdraft_is_rejected_and_balance_unchanged() {
    let Some(pg) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let user = hustle_testkit::make_user(&pg, "gold").await;
    let ledger = PgGoldLedger::new(pg.clone());

    ledger
        .grant(op(user, 1_000, GoldReason::Welcome, None))
        .await
        .unwrap();
    let err = ledger
        .grant(op(user, -2_000, GoldReason::CosmeticPurchase, None))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LedgerError::Insufficient {
            balance: 1000,
            needed: 2000
        }
    ));
    assert_eq!(ledger.balance(user).await.unwrap(), 1_000);
}

#[tokio::test]
async fn sign_rules_reject_wrong_directions() {
    let Some(pg) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let user = hustle_testkit::make_user(&pg, "gold").await;
    let ledger = PgGoldLedger::new(pg.clone());

    // Welcome must be positive.
    assert!(matches!(
        ledger.grant(op(user, -1, GoldReason::Welcome, None)).await,
        Err(LedgerError::BadDelta { .. })
    ));
    // Purchases must be negative.
    assert!(matches!(
        ledger
            .grant(op(user, 100, GoldReason::CosmeticPurchase, None))
            .await,
        Err(LedgerError::BadDelta { .. })
    ));
    // Zero delta is never a valid op.
    assert!(matches!(
        ledger.grant(op(user, 0, GoldReason::Gameplay, None)).await,
        Err(LedgerError::BadDelta { .. })
    ));
}

#[tokio::test]
async fn page_returns_newest_first_with_cursor() {
    let Some(pg) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let user = hustle_testkit::make_user(&pg, "gold").await;
    let ledger = PgGoldLedger::new(pg.clone());

    for i in 1..=5 {
        ledger
            .grant(op(
                user,
                100 * i,
                GoldReason::Gameplay,
                Some(&format!("gp:{i}")),
            ))
            .await
            .unwrap();
    }
    let page1 = ledger.page(user, None, 3).await.unwrap();
    assert_eq!(page1.len(), 3);
    assert_eq!(page1[0].delta, 500); // newest first
    assert!(page1[0].id > page1[1].id);

    let page2 = ledger.page(user, Some(page1[2].id), 3).await.unwrap();
    assert_eq!(page2.len(), 2);
    assert_eq!(page2[1].delta, 100);

    // Total balance sanity: 100+200+300+400+500
    assert_eq!(ledger.balance(user).await.unwrap(), 1_500);
}
