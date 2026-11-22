//! Pairing sweep algorithms (ADR-004, overview.md §4.2).

use crate::queue::QueueEntry;

/// Calculate pairing rating window for a player based on wait time and mode.
/// Rated: min(75 + 15 * wait_s, 700); provisional: 400 flat; casual: 1000 flat.
pub fn rating_window(entry: &QueueEntry, now_unix_s: i64) -> f64 {
    if !entry.mode.is_rated() {
        return 1000.0;
    }
    if entry.provisional {
        return 400.0;
    }
    let wait_s = (now_unix_s - entry.enqueued_at_unix_s).max(0) as f64;
    (75.0 + 15.0 * wait_s).min(700.0)
}

/// Check if two queued players can be matched based on ratings and rating windows.
pub fn can_pair(p: &QueueEntry, q: &QueueEntry, now_unix_s: i64) -> bool {
    if p.user_id == q.user_id {
        return false;
    }
    if p.pool != q.pool || p.mode != q.mode {
        return false;
    }

    let diff = (p.rating - q.rating).abs();
    let win_p = rating_window(p, now_unix_s);
    let win_q = rating_window(q, now_unix_s);
    diff <= win_p.min(win_q)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queue::QueueMode;
    use hustle_domain::{Pool, TimeControl};

    fn make_test_entry(
        user_id: i64,
        rating: f64,
        provisional: bool,
        enqueued_at: i64,
        mode: QueueMode,
    ) -> QueueEntry {
        QueueEntry {
            user_id,
            public_id: format!("01JDMK{user_id}"),
            handle: format!("player_{user_id}"),
            pool: Pool::Blitz,
            mode,
            tc: TimeControl {
                initial_ms: 180000,
                increment_ms: 2000,
            },
            rating,
            rd: 80.0,
            provisional,
            enqueued_at_unix_s: enqueued_at,
        }
    }

    #[test]
    fn rating_window_expansion() {
        let entry = make_test_entry(1, 1500.0, false, 100, QueueMode::Rated);
        // At 0 wait seconds
        assert_eq!(rating_window(&entry, 100), 75.0);
        // At 10 wait seconds: 75 + 15*10 = 225
        assert_eq!(rating_window(&entry, 110), 225.0);
        // At 50 wait seconds: 75 + 15*50 = 825 clamped to 700
        assert_eq!(rating_window(&entry, 150), 700.0);
    }

    #[test]
    fn provisional_and_casual_windows() {
        let prov = make_test_entry(1, 1500.0, true, 100, QueueMode::Rated);
        assert_eq!(rating_window(&prov, 100), 400.0);
        assert_eq!(rating_window(&prov, 200), 400.0);

        let casual = make_test_entry(2, 1500.0, false, 100, QueueMode::Casual);
        assert_eq!(rating_window(&casual, 100), 1000.0);
        assert_eq!(rating_window(&casual, 200), 1000.0);
    }

    #[test]
    fn compatibility_checks() {
        let now = 100;
        let p1 = make_test_entry(1, 1500.0, false, now, QueueMode::Rated);
        let p2_close = make_test_entry(2, 1550.0, false, now, QueueMode::Rated); // diff 50 <= 75
        let p3_far = make_test_entry(3, 1600.0, false, now, QueueMode::Rated); // diff 100 > 75

        assert!(can_pair(&p1, &p2_close, now));
        assert!(!can_pair(&p1, &p3_far, now));

        // After 10s wait, window expands to 225
        assert!(can_pair(&p1, &p3_far, now + 10));
    }
}
