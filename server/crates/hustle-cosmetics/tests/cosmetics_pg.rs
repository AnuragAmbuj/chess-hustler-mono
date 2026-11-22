use hustle_cosmetics::{CosmeticsError, CosmeticsService, UpdateLoadoutRequest};
use hustle_identity::signup::{Consents, ProfileInput, SignupService, VerifiedIdentity};
use hustle_identity::TokenService;
use std::sync::atomic::{AtomicU64, Ordering};

fn counter() -> u64 {
    static N: AtomicU64 = AtomicU64::new(20_000);
    N.fetch_add(1, Ordering::Relaxed)
        + (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            % 10_000_000)
}

fn phone() -> String {
    format!("+9198{:010}", counter())
}

fn handle() -> String {
    format!("cosm_user_{}", counter())
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

async fn make_user(pool: &sqlx::PgPool, welcome_gold: i64) -> i64 {
    let token_service = TokenService::new(b"test_secret_key_32_bytes_long_ok!");
    let signup = SignupService::new(pool.clone(), token_service, welcome_gold);
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

    outcome.user_id
}

#[tokio::test]
async fn test_list_catalog_and_category_filtering() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let svc = CosmeticsService::new(pool.clone());
    let user_id = make_user(&pool, 1_000).await;

    // 1. List all active items
    let all_items = svc.list_catalog(Some(user_id), None).await.unwrap();
    assert!(!all_items.is_empty(), "catalog should have seeded items");

    // 2. Filter by 'board'
    let boards = svc
        .list_catalog(Some(user_id), Some("board"))
        .await
        .unwrap();
    assert!(boards.iter().all(|i| i.category == "board"));
    assert!(boards.iter().any(|i| i.id == "board_chaturanga"));

    // 3. Filter by 'avatar'
    let avatars = svc
        .list_catalog(Some(user_id), Some("avatar"))
        .await
        .unwrap();
    assert!(avatars.iter().all(|i| i.category == "avatar"));

    // 4. Verify none owned yet
    assert!(all_items.iter().all(|i| !i.owned));
}

#[tokio::test]
async fn test_purchase_insufficient_funds() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let svc = CosmeticsService::new(pool.clone());
    let user_id = make_user(&pool, 100).await; // only 100 gold

    let res = svc.purchase(user_id, "board_chaturanga", None).await;
    match res {
        Err(CosmeticsError::Ledger(hustle_gold::LedgerError::Insufficient { balance, needed })) => {
            assert_eq!(balance, 100);
            assert_eq!(needed, 30_000);
        }
        other => panic!("expected Insufficient funds error, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_purchase_success_and_idempotency() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let svc = CosmeticsService::new(pool.clone());
    let user_id = make_user(&pool, 50_000).await;

    let idem_key = format!("idem_purchase_{}", counter());

    // 1. First purchase succeeds
    let res = svc
        .purchase(user_id, "board_chaturanga", Some(&idem_key))
        .await
        .expect("purchase should succeed");
    assert!(res.owned);
    assert_eq!(res.balance, 20_000);
    assert_eq!(res.ledger.delta, -30_000);
    assert!(!res.replayed);

    // 2. Catalog and detail shows owned = true
    let item = svc
        .get_item(Some(user_id), "board_chaturanga")
        .await
        .unwrap();
    assert!(item.owned);

    // 3. Replay with same idem_key succeeds idempotently
    let replay = svc
        .purchase(user_id, "board_chaturanga", Some(&idem_key))
        .await
        .expect("replay should succeed idempotently");
    assert!(replay.owned);
    assert_eq!(replay.balance, 20_000);
    assert!(replay.replayed);

    // 4. Attempt with different idem_key -> AlreadyOwned
    let different_key = format!("idem_purchase_{}", counter());
    let err = svc
        .purchase(user_id, "board_chaturanga", Some(&different_key))
        .await
        .unwrap_err();
    match err {
        CosmeticsError::AlreadyOwned(id) => assert_eq!(id, "board_chaturanga"),
        other => panic!("expected AlreadyOwned, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_loadout_management_and_validation() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let svc = CosmeticsService::new(pool.clone());
    let user_id = make_user(&pool, 50_000).await;

    // 1. Default loadout is empty
    let loadout = svc.get_loadout(user_id).await.unwrap();
    assert!(loadout.board_id.is_none());
    assert!(loadout.avatar_id.is_none());

    // 2. Equipping unowned cosmetic fails with NotOwned
    let err = svc
        .update_loadout(
            user_id,
            UpdateLoadoutRequest {
                board_id: Some("board_chaturanga".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    match err {
        CosmeticsError::NotOwned(id) => assert_eq!(id, "board_chaturanga"),
        other => panic!("expected NotOwned, got: {:?}", other),
    }

    // 3. Buy board_chaturanga and avatar_street_hustler
    svc.purchase(user_id, "board_chaturanga", None)
        .await
        .unwrap();
    svc.purchase(user_id, "avatar_street_hustler", None)
        .await
        .unwrap();

    // 4. Equipping board into avatar slot fails with InvalidCategory
    let err = svc
        .update_loadout(
            user_id,
            UpdateLoadoutRequest {
                avatar_id: Some("board_chaturanga".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    match err {
        CosmeticsError::InvalidCategory {
            item_id,
            expected,
            actual,
        } => {
            assert_eq!(item_id, "board_chaturanga");
            assert_eq!(expected, "avatar");
            assert_eq!(actual, "board");
        }
        other => panic!("expected InvalidCategory, got: {:?}", other),
    }

    // 5. Valid loadout equip succeeds
    let updated = svc
        .update_loadout(
            user_id,
            UpdateLoadoutRequest {
                board_id: Some("board_chaturanga".to_string()),
                avatar_id: Some("avatar_street_hustler".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert_eq!(updated.board_id.as_deref(), Some("board_chaturanga"));
    assert_eq!(updated.avatar_id.as_deref(), Some("avatar_street_hustler"));

    // Profile avatar was also synchronized
    let profile_avatar: Option<String> =
        sqlx::query_scalar("SELECT avatar_cosmetic_id FROM profiles WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(profile_avatar.as_deref(), Some("avatar_street_hustler"));

    // 6. Clearing avatar slot
    let cleared = svc
        .update_loadout(
            user_id,
            UpdateLoadoutRequest {
                avatar_id: Some("".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert_eq!(cleared.board_id.as_deref(), Some("board_chaturanga"));
    assert!(cleared.avatar_id.is_none());
}
