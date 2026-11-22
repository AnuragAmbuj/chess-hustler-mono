use crate::domain::{StandingItem, TournamentPairing};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct PlayerState {
    pub user_id: i64,
    pub points: f32,
    pub wins: i32,
    pub draws: i32,
    pub losses: i32,
    pub opponents: Vec<i64>,
    pub colors: Vec<bool>, // true = White, false = Black
    pub has_had_bye: bool,
}

impl PlayerState {
    pub fn new(user_id: i64) -> Self {
        Self {
            user_id,
            points: 0.0,
            wins: 0,
            draws: 0,
            losses: 0,
            opponents: Vec::new(),
            colors: Vec::new(),
            has_had_bye: false,
        }
    }

    pub fn white_count(&self) -> usize {
        self.colors.iter().filter(|&&c| c).count()
    }

    pub fn black_count(&self) -> usize {
        self.colors.iter().filter(|&&c| !c).count()
    }
}

/// Computes current player states and standings from all finished pairings.
pub fn compute_standings(
    players: &[i64],
    handles: &HashMap<i64, String>,
    pairings: &[TournamentPairing],
) -> (HashMap<i64, PlayerState>, Vec<StandingItem>) {
    let mut states: HashMap<i64, PlayerState> = players
        .iter()
        .map(|&uid| (uid, PlayerState::new(uid)))
        .collect();

    for p in pairings {
        match (p.white_id, p.black_id, p.result.as_deref()) {
            (Some(w), Some(b), Some("white") | Some("forfeit_black")) => {
                if let Some(ws) = states.get_mut(&w) {
                    ws.points += 1.0;
                    ws.wins += 1;
                    ws.opponents.push(b);
                    ws.colors.push(true);
                }
                if let Some(bs) = states.get_mut(&b) {
                    bs.losses += 1;
                    bs.opponents.push(w);
                    bs.colors.push(false);
                }
            }
            (Some(w), Some(b), Some("black") | Some("forfeit_white")) => {
                if let Some(bs) = states.get_mut(&b) {
                    bs.points += 1.0;
                    bs.wins += 1;
                    bs.opponents.push(w);
                    bs.colors.push(false);
                }
                if let Some(ws) = states.get_mut(&w) {
                    ws.losses += 1;
                    ws.opponents.push(b);
                    ws.colors.push(true);
                }
            }
            (Some(w), Some(b), Some("draw")) => {
                if let Some(ws) = states.get_mut(&w) {
                    ws.points += 0.5;
                    ws.draws += 1;
                    ws.opponents.push(b);
                    ws.colors.push(true);
                }
                if let Some(bs) = states.get_mut(&b) {
                    bs.points += 0.5;
                    bs.draws += 1;
                    bs.opponents.push(w);
                    bs.colors.push(false);
                }
            }
            (Some(uid), None, Some("bye")) => {
                if let Some(s) = states.get_mut(&uid) {
                    s.points += 1.0;
                    s.wins += 1;
                    s.has_had_bye = true;
                }
            }
            _ => {}
        }
    }

    // Calculate Buchholz and Sonneborn-Berger
    let mut standings = Vec::new();
    for (&uid, state) in &states {
        let mut buchholz = 0.0f32;
        let mut sb = 0.0f32;

        for &opp_id in &state.opponents {
            if let Some(opp_state) = states.get(&opp_id) {
                buchholz += opp_state.points;
                // Sonneborn-Berger: add full score if won, half score if drawn
                for p in pairings {
                    if (p.white_id == Some(uid) && p.black_id == Some(opp_id))
                        || (p.black_id == Some(uid) && p.white_id == Some(opp_id))
                    {
                        let won = (p.white_id == Some(uid) && p.result.as_deref() == Some("white"))
                            || (p.black_id == Some(uid) && p.result.as_deref() == Some("black"));
                        let drawn = p.result.as_deref() == Some("draw");
                        if won {
                            sb += opp_state.points;
                        } else if drawn {
                            sb += opp_state.points * 0.5;
                        }
                    }
                }
            }
        }

        let handle = handles
            .get(&uid)
            .cloned()
            .unwrap_or_else(|| format!("user_{uid}"));

        let games_played = state.wins + state.draws + state.losses;
        standings.push(StandingItem {
            rank: 0,
            user_id: uid,
            handle,
            points: state.points,
            buchholz,
            sonneborn_berger: sb,
            games_played,
            wins: state.wins,
            draws: state.draws,
            losses: state.losses,
        });
    }

    // Sort: points DESC, Buchholz DESC, Sonneborn-Berger DESC, user_id ASC
    standings.sort_by(|a, b| {
        b.points
            .partial_cmp(&a.points)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                b.buchholz
                    .partial_cmp(&a.buchholz)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                b.sonneborn_berger
                    .partial_cmp(&a.sonneborn_berger)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.user_id.cmp(&b.user_id))
    });

    for (idx, item) in standings.iter_mut().enumerate() {
        item.rank = (idx + 1) as i32;
    }

    (states, standings)
}

/// Generates Swiss pairings for the next round.
pub fn generate_next_round_pairings(
    tournament_id: i64,
    round: i32,
    active_players: &[i64],
    past_pairings: &[TournamentPairing],
) -> Vec<TournamentPairing> {
    if active_players.is_empty() {
        return Vec::new();
    }

    let dummy_handles = HashMap::new();
    let (states, standings) = compute_standings(active_players, &dummy_handles, past_pairings);

    // Track played opponents set per player
    let mut played_map: HashMap<i64, HashSet<i64>> = HashMap::new();
    for p in past_pairings {
        if let (Some(w), Some(b)) = (p.white_id, p.black_id) {
            played_map.entry(w).or_default().insert(b);
            played_map.entry(b).or_default().insert(w);
        }
    }

    let mut pool: Vec<i64> = standings.iter().map(|s| s.user_id).collect();
    let mut round_pairings = Vec::new();
    let mut board = 1;

    // Handle odd player count: assign Bye to lowest-ranked player without a bye
    if !pool.len().is_multiple_of(2) {
        if let Some(bye_idx) = pool.iter().rposition(|&uid| {
            states.get(&uid).map(|s| !s.has_had_bye).unwrap_or(true)
        }) {
            let bye_player = pool.remove(bye_idx);
            round_pairings.push(TournamentPairing {
                id: 0,
                tournament_id,
                round,
                board,
                white_id: Some(bye_player),
                black_id: None,
                game_id: None,
                result: Some("bye".to_string()),
            });
            board += 1;
        }
    }

    // Greedy score-bracket Swiss pairing
    let mut paired = HashSet::new();
    for i in 0..pool.len() {
        let p1 = pool[i];
        if paired.contains(&p1) {
            continue;
        }

        let p1_played = played_map.get(&p1);
        let mut candidate_idx = None;

        // Find highest scoring available candidate who hasn't played p1
        for (offset, &p2) in pool[(i + 1)..].iter().enumerate() {
            if paired.contains(&p2) {
                continue;
            }

            let already_played = p1_played.map(|set| set.contains(&p2)).unwrap_or(false);
            if !already_played {
                candidate_idx = Some(i + 1 + offset);
                break;
            }
        }

        // Fallback: if all available opponents have been played, pair with first available
        let opponent = match candidate_idx {
            Some(idx) => pool[idx],
            None => {
                pool.iter()
                    .skip(i + 1)
                    .find(|&&p| !paired.contains(&p))
                    .copied()
                    .unwrap_or(p1)
            }
        };

        if opponent == p1 {
            // Cannot pair with self
            continue;
        }

        paired.insert(p1);
        paired.insert(opponent);

        // Color allocation balance
        let s1 = states.get(&p1);
        let s2 = states.get(&opponent);
        let p1_whites = s1.map(|s| s.white_count()).unwrap_or(0);
        let p2_whites = s2.map(|s| s.white_count()).unwrap_or(0);

        let (white_id, black_id) = if p1_whites > p2_whites {
            (opponent, p1)
        } else {
            (p1, opponent)
        };

        round_pairings.push(TournamentPairing {
            id: 0,
            tournament_id,
            round,
            board,
            white_id: Some(white_id),
            black_id: Some(black_id),
            game_id: None,
            result: None,
        });
        board += 1;
    }

    round_pairings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_swiss_first_round_pairings_4_players() {
        let players = vec![1, 2, 3, 4];
        let pairings = generate_next_round_pairings(100, 1, &players, &[]);

        assert_eq!(pairings.len(), 2);
        assert_eq!(pairings[0].round, 1);
        assert_eq!(pairings[0].board, 1);
        assert_eq!(pairings[1].board, 2);

        let paired_players: HashSet<i64> = pairings
            .iter()
            .flat_map(|p| vec![p.white_id.unwrap(), p.black_id.unwrap()])
            .collect();
        assert_eq!(paired_players.len(), 4);
    }

    #[test]
    fn test_swiss_odd_players_receives_bye() {
        let players = vec![1, 2, 3];
        let pairings = generate_next_round_pairings(100, 1, &players, &[]);

        assert_eq!(pairings.len(), 2);
        let bye_pairing = pairings.iter().find(|p| p.result.as_deref() == Some("bye"));
        assert!(bye_pairing.is_some());
        let bye = bye_pairing.unwrap();
        assert!(bye.white_id.is_some());
        assert!(bye.black_id.is_none());
    }

    #[test]
    fn test_swiss_no_rematch_in_subsequent_rounds() {
        let players = vec![1, 2, 3, 4];

        // Round 1: 1 vs 2 (1 wins), 3 vs 4 (3 wins)
        let round_1 = vec![
            TournamentPairing {
                id: 1,
                tournament_id: 100,
                round: 1,
                board: 1,
                white_id: Some(1),
                black_id: Some(2),
                game_id: Some(10),
                result: Some("white".into()),
            },
            TournamentPairing {
                id: 2,
                tournament_id: 100,
                round: 1,
                board: 2,
                white_id: Some(3),
                black_id: Some(4),
                game_id: Some(11),
                result: Some("white".into()),
            },
        ];

        // Round 2 pairing: Winners 1 & 3 should play each other, losers 2 & 4 should play each other!
        let round_2 = generate_next_round_pairings(100, 2, &players, &round_1);

        assert_eq!(round_2.len(), 2);
        let board_1 = &round_2.iter().find(|p| p.board == 1).unwrap();
        let board_1_players: HashSet<i64> = [board_1.white_id.unwrap(), board_1.black_id.unwrap()]
            .into_iter()
            .collect();

        // 1 and 3 should be on top board (both have 1.0 point)
        assert!(board_1_players.contains(&1));
        assert!(board_1_players.contains(&3));
    }

    #[test]
    fn test_buchholz_and_standings_calculation() {
        let players = vec![1, 2, 3, 4];
        let mut handles = HashMap::new();
        handles.insert(1, "alice".into());
        handles.insert(2, "bob".into());
        handles.insert(3, "charlie".into());
        handles.insert(4, "david".into());

        let finished_games = vec![
            TournamentPairing {
                id: 1,
                tournament_id: 100,
                round: 1,
                board: 1,
                white_id: Some(1),
                black_id: Some(2),
                game_id: Some(10),
                result: Some("white".into()),
            },
            TournamentPairing {
                id: 2,
                tournament_id: 100,
                round: 1,
                board: 2,
                white_id: Some(3),
                black_id: Some(4),
                game_id: Some(11),
                result: Some("draw".into()),
            },
        ];

        let (_, standings) = compute_standings(&players, &handles, &finished_games);
        assert_eq!(standings[0].user_id, 1);
        assert_eq!(standings[0].points, 1.0);
        assert_eq!(standings[0].rank, 1);
    }
}
