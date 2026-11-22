use hustle_friends::{ChallengeService, FriendService};
use hustle_realtime::EventPublisher;
use sqlx::Row;

#[tokio::test]
async fn test_friend_request_and_accept_lifecycle_pg() {
    let Some(pg) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let Some(redis) = hustle_testkit::redis().await else {
        eprintln!("skip: no CH_TEST_REDIS_URL");
        return;
    };

    let publisher = EventPublisher::new(redis.clone());
    let service = FriendService::new(pg.clone(), redis.clone(), publisher);

    let (user_a, user_b) = hustle_testkit::make_two_users(&pg, "fr1").await;

    // Get user B's handle
    let handle_b: String = sqlx::query_scalar("SELECT handle FROM profiles WHERE user_id = $1")
        .bind(user_b)
        .fetch_one(&pg)
        .await
        .unwrap();

    // 1. User A sends friend request to User B
    let req = service.send_request(user_a, &handle_b).await.unwrap();
    assert_eq!(req.handle, handle_b);

    // 2. User B checks pending requests
    let b_requests = service.get_requests(user_b).await.unwrap();
    assert_eq!(b_requests.incoming.len(), 1);
    assert_eq!(b_requests.incoming[0].id, req.id);
    assert_eq!(b_requests.outgoing.len(), 0);

    // User A checks pending requests
    let a_requests = service.get_requests(user_a).await.unwrap();
    assert_eq!(a_requests.outgoing.len(), 1);
    assert_eq!(a_requests.outgoing[0].id, req.id);
    assert_eq!(a_requests.incoming.len(), 0);

    // 3. User B accepts request
    service.accept_request(user_b, req.id).await.unwrap();

    // 4. Both User A and User B should now list each other as friends
    let a_friends = service.get_friends(user_a).await.unwrap();
    assert_eq!(a_friends.len(), 1);
    assert_eq!(a_friends[0].handle, handle_b);

    let b_friends = service.get_friends(user_b).await.unwrap();
    assert_eq!(b_friends.len(), 1);

    // 5. Duplicate request is rejected
    assert!(service.send_request(user_a, &handle_b).await.is_err());

    // 6. User A removes friend
    service.remove_friend(user_a, req.id).await.unwrap();
    let a_friends_after = service.get_friends(user_a).await.unwrap();
    assert_eq!(a_friends_after.len(), 0);
}

#[tokio::test]
async fn test_mutual_request_auto_accepts() {
    let Some(pg) = hustle_testkit::pool().await else {
        return;
    };
    let Some(redis) = hustle_testkit::redis().await else {
        return;
    };

    let publisher = EventPublisher::new(redis.clone());
    let service = FriendService::new(pg.clone(), redis.clone(), publisher);

    let (user_a, user_b) = hustle_testkit::make_two_users(&pg, "mut").await;
    let handle_a: String = sqlx::query_scalar("SELECT handle FROM profiles WHERE user_id = $1")
        .bind(user_a)
        .fetch_one(&pg)
        .await
        .unwrap();
    let handle_b: String = sqlx::query_scalar("SELECT handle FROM profiles WHERE user_id = $1")
        .bind(user_b)
        .fetch_one(&pg)
        .await
        .unwrap();

    // A requests B
    service.send_request(user_a, &handle_b).await.unwrap();

    // B requests A (mutual request -> auto-accepts!)
    service.send_request(user_b, &handle_a).await.unwrap();

    // Both should now be friends
    let friends = service.get_friends(user_a).await.unwrap();
    assert_eq!(friends.len(), 1);
    assert_eq!(friends[0].handle, handle_b);
}

#[tokio::test]
async fn test_block_and_unblock_lifecycle() {
    let Some(pg) = hustle_testkit::pool().await else {
        return;
    };
    let Some(redis) = hustle_testkit::redis().await else {
        return;
    };

    let publisher = EventPublisher::new(redis.clone());
    let service = FriendService::new(pg.clone(), redis.clone(), publisher);

    let (user_a, user_b) = hustle_testkit::make_two_users(&pg, "blk").await;
    let handle_b: String = sqlx::query_scalar("SELECT handle FROM profiles WHERE user_id = $1")
        .bind(user_b)
        .fetch_one(&pg)
        .await
        .unwrap();

    // 1. Block User B
    let blocked = service.block_user(user_a, &handle_b).await.unwrap();
    assert_eq!(blocked.handle, handle_b);

    let blocks = service.get_blocks(user_a).await.unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].handle, handle_b);

    // 2. Friend request between blocked users is prohibited
    assert!(service.send_request(user_a, &handle_b).await.is_err());
    assert!(service.send_request(user_b, &handle_b).await.is_err()); // self action also rejected

    let handle_a: String = sqlx::query_scalar("SELECT handle FROM profiles WHERE user_id = $1")
        .bind(user_a)
        .fetch_one(&pg)
        .await
        .unwrap();
    assert!(service.send_request(user_b, &handle_a).await.is_err());

    // 3. Unblock User B
    service.unblock_user(user_a, &handle_b).await.unwrap();
    let blocks_after = service.get_blocks(user_a).await.unwrap();
    assert_eq!(blocks_after.len(), 0);

    // 4. Friend request succeeds after unblock
    assert!(service.send_request(user_a, &handle_b).await.is_ok());
}

#[tokio::test]
async fn test_direct_challenge_lifecycle() {
    let Some(pg) = hustle_testkit::pool().await else {
        return;
    };
    let Some(redis) = hustle_testkit::redis().await else {
        return;
    };

    let publisher = EventPublisher::new(redis.clone());
    let game_service = hustle_chess::GameService::new(pg.clone());
    let challenge_service =
        ChallengeService::new(pg.clone(), redis.clone(), publisher, game_service);

    let (user_a, user_b) = hustle_testkit::make_two_users(&pg, "chal").await;
    let handle_b: String = sqlx::query_scalar("SELECT handle FROM profiles WHERE user_id = $1")
        .bind(user_b)
        .fetch_one(&pg)
        .await
        .unwrap();

    // 1. Create a 3+2 rated challenge
    let challenge = challenge_service
        .create_challenge(user_a, &handle_b, 180, 2, true)
        .await
        .unwrap();

    assert_eq!(challenge.initial_s, 180);
    assert_eq!(challenge.increment_s, 2);
    assert!(challenge.rated);

    // 2. Target accepts challenge -> spawns game
    let game_id = challenge_service
        .accept_challenge(user_b, &challenge.id)
        .await
        .unwrap();

    // Verify game was created in Postgres
    let game_row =
        sqlx::query("SELECT id, public_id, status, pool FROM games WHERE public_id = $1")
            .bind(&game_id)
            .fetch_one(&pg)
            .await
            .unwrap();

    let status: String = game_row.get("status");
    let pool: String = game_row.get("pool");
    assert_eq!(status, "starting");
    assert_eq!(pool, "blitz");
}
