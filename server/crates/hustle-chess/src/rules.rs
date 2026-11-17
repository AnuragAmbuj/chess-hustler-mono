//! Pure chess rules engine — a thin, total wrapper over `shakmaty`.
//!
//! Server-authoritative (ADR-003): every legality/result decision the platform
//! makes flows through [`RulesEngine`]. FEN in → UCI/SAN/FEN/outcome out.
//! Repetition detection uses FEN-prefix keys (placement, turn, castling, legal
//! ep) per standard practice; fifty-move uses the halfmove clock ≥ 100.

use serde::{Deserialize, Serialize};
use shakmaty::san::San;
use shakmaty::uci::UciMove;
use shakmaty::{fen::Fen, Board, CastlingMode, Chess, Color, EnPassantMode, Position, Role};

/// The platform's only starting position.
pub const STARTING_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// Termination reasons — string values MUST match the `games.termination`
/// CHECK constraint (`server/migrations/0001_init.sql`) and lifecycle §2.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Termination {
    Checkmate,
    Stalemate,
    Resignation,
    Timeout,
    InsufficientMaterial,
    Agreement,
    Threefold,
    FiftyMove,
    Aborted,
    Abandoned,
    Forfeit,
}

impl Termination {
    pub fn as_str(self) -> &'static str {
        match self {
            Termination::Checkmate => "checkmate",
            Termination::Stalemate => "stalemate",
            Termination::Resignation => "resignation",
            Termination::Timeout => "timeout",
            Termination::InsufficientMaterial => "insufficient_material",
            Termination::Agreement => "agreement",
            Termination::Threefold => "threefold",
            Termination::FiftyMove => "fifty_move",
            Termination::Aborted => "aborted",
            Termination::Abandoned => "abandoned",
            Termination::Forfeit => "forfeit",
        }
    }
}

/// Server-evaluated terminal outcome (rules-detected only: mate/stalemate/
/// insufficient/50-move/threefold). Resignation, timeout and abort are decided
/// by the service layer with the same `Termination` vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoOutcome {
    Ongoing,
    WhiteWin(Termination),
    BlackWin(Termination),
    Draw(Termination),
}

impl AutoOutcome {
    /// `(result, termination)` for finished games, matching the DB CHECK
    /// vocabulary ('white' | 'black' | 'draw').
    pub fn terminal(self) -> Option<(&'static str, Termination)> {
        match self {
            AutoOutcome::Ongoing => None,
            AutoOutcome::WhiteWin(t) => Some(("white", t)),
            AutoOutcome::BlackWin(t) => Some(("black", t)),
            AutoOutcome::Draw(t) => Some(("draw", t)),
        }
    }
}

/// Parse errors surfaced to clients as `game.illegal_move` / validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleError {
    BadFen,
    BadUci,
    IllegalMove,
}

impl std::fmt::Display for RuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuleError::BadFen => f.write_str("invalid FEN"),
            RuleError::BadUci => f.write_str("invalid UCI move"),
            RuleError::IllegalMove => f.write_str("illegal move"),
        }
    }
}

/// One played move, fully resolved by the server (never trusted from client).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayedMove {
    pub uci: String,
    pub san: String,
    pub fen_after: String,
}

/// Pure rules facade. Holds the current position plus repetition keys.
#[derive(Debug, Clone)]
pub struct RulesEngine {
    pos: Chess,
    /// FEN 4-field prefixes of every position reached (incl. start), oldest first.
    keys: Vec<String>,
    /// Plies played so far (server-numbered; 0 = game start).
    ply: u32,
}

impl RulesEngine {
    pub fn from_fen(fen: &str) -> Result<Self, RuleError> {
        Self::from_fen_with_keys(fen, Vec::new())
    }

    /// Rebuild mid-game (crash recovery / reconnection): historical keys are
    /// recomputed by the caller from `moves.fen_after` rows.
    pub fn from_fen_with_keys(fen: &str, prior_keys: Vec<String>) -> Result<Self, RuleError> {
        let parsed: Fen = fen.parse().map_err(|_| RuleError::BadFen)?;
        let pos = parsed
            .into_position::<Chess>(CastlingMode::Standard)
            .map_err(|_| RuleError::BadFen)?;
        let mut keys = prior_keys;
        keys.push(Self::key_of(&pos));
        Ok(Self { pos, keys, ply: 0 })
    }

    pub fn starting() -> Self {
        Self::from_fen(STARTING_FEN).expect("STARTING_FEN is valid")
    }

    pub fn fen(&self) -> String {
        Fen::from_position(&self.pos, EnPassantMode::Legal).to_string()
    }

    pub fn ply(&self) -> u32 {
        self.ply
    }

    pub fn to_move(&self) -> Color {
        self.pos.turn()
    }

    pub fn board(&self) -> &Board {
        self.pos.board()
    }

    /// Parse + validate a UCI move against the position, apply it, and
    /// evaluate server-detected outcomes. Rejects anything illegal.
    pub fn play_uci(&mut self, uci_str: &str) -> Result<(PlayedMove, AutoOutcome), RuleError> {
        let uci_in: UciMove = uci_str.parse().map_err(|_| RuleError::BadUci)?;
        let mv = uci_in
            .to_move(&self.pos)
            .map_err(|_| RuleError::IllegalMove)?;
        let san = San::from_move(&self.pos, mv).to_string();

        self.pos.play_unchecked(mv);
        self.ply += 1;
        let fen_after = self.fen();
        self.keys.push(Self::key_of(&self.pos));

        let uci = UciMove::from_move(mv, CastlingMode::Standard).to_string();
        let outcome = self.evaluate_outcome();
        Ok((
            PlayedMove {
                uci,
                san,
                fen_after,
            },
            outcome,
        ))
    }

    /// Server-detected terminal outcome for the position just played.
    pub fn evaluate_outcome(&self) -> AutoOutcome {
        if self.pos.is_checkmate() {
            return match self.pos.turn() {
                Color::White => AutoOutcome::BlackWin(Termination::Checkmate), // to-move is mated
                Color::Black => AutoOutcome::WhiteWin(Termination::Checkmate),
            };
        }
        if self.pos.is_stalemate() {
            return AutoOutcome::Draw(Termination::Stalemate);
        }
        if self.pos.is_insufficient_material() {
            return AutoOutcome::Draw(Termination::InsufficientMaterial);
        }
        if self.pos.halfmoves() >= 100 {
            return AutoOutcome::Draw(Termination::FiftyMove);
        }
        if self.repetition_count() >= 3 {
            return AutoOutcome::Draw(Termination::Threefold);
        }
        AutoOutcome::Ongoing
    }

    /// Occurrences of the current position among the history keys.
    pub fn repetition_count(&self) -> u32 {
        let cur = Self::key_of(&self.pos);
        self.keys.iter().filter(|k| **k == cur).count() as u32
    }

    /// FEN prefix key for repetition detection: placement, turn, castling, ep.
    fn key_of(pos: &Chess) -> String {
        let fen = Fen::from_position(pos, EnPassantMode::Legal).to_string();
        let mut key = String::with_capacity(80);
        for field in fen.split_whitespace().take(4) {
            key.push_str(field);
            key.push(' ');
        }
        key
    }

    /// Mate-capability approximation for timeout resolution (lifecycle §3,
    /// property-tested): `side` can mate iff it holds a pawn/rook/queen, ≥ 2
    /// minors, or exactly one minor while the opponent has any non-king piece
    /// (helpmate possibility).
    pub fn can_potentially_mate(board: &Board, side: Color, opponent: Color) -> bool {
        let mine = board.by_color(side);
        let heavy =
            board.by_role(Role::Pawn) | board.by_role(Role::Rook) | board.by_role(Role::Queen);
        if (mine & heavy).any() {
            return true;
        }
        let minors = (mine & board.by_role(Role::Knight)).count()
            + (mine & board.by_role(Role::Bishop)).count();
        if minors >= 2 {
            return true;
        }
        if minors == 1 {
            let opponent_non_king = board.by_color(opponent) & !board.kings();
            return opponent_non_king.any();
        }
        false
    }

    /// Timeout decision at a flag: `to_move` flagged; returns the terminal
    /// outcome (win for opponent, or draw when opponent cannot mate).
    pub fn flag_outcome(&self, to_move: Color) -> AutoOutcome {
        let opponent = match to_move {
            Color::White => Color::Black,
            Color::Black => Color::White,
        };
        let opp_can_mate = Self::can_potentially_mate(self.pos.board(), opponent, to_move);
        if opp_can_mate {
            match to_move {
                Color::White => AutoOutcome::BlackWin(Termination::Timeout),
                Color::Black => AutoOutcome::WhiteWin(Termination::Timeout),
            }
        } else {
            AutoOutcome::Draw(Termination::Timeout)
        }
    }

    /// Dead-position check (K vs K, K+N vs K, K+B vs K, same-colored Bishops).
    pub fn is_insufficient_material(&self) -> bool {
        self.pos.is_insufficient_material()
    }

    /// Legal UCI list (tests and future validation endpoints).
    pub fn legal_ucis(&self) -> Vec<String> {
        self.pos
            .legal_moves()
            .iter()
            .map(|mv| UciMove::from_move(*mv, CastlingMode::Standard).to_string())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine(fen: &str) -> RulesEngine {
        RulesEngine::from_fen(fen).expect("test FEN must be valid")
    }

    #[test]
    fn starting_position_has_twenty_legal_moves() {
        let e = RulesEngine::starting();
        assert_eq!(e.legal_ucis().len(), 20);
        assert_eq!(e.to_move(), Color::White);
        assert_eq!(e.fen(), STARTING_FEN);
        assert_eq!(e.evaluate_outcome(), AutoOutcome::Ongoing);
    }

    #[test]
    fn open_game_via_uci_then_rejects_illegal() {
        let mut e = RulesEngine::starting();
        let (mv, outcome) = e.play_uci("e2e4").expect("e4 legal");
        assert_eq!(mv.uci, "e2e4");
        assert_eq!(mv.san, "e4");
        assert_eq!(
            mv.fen_after,
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1"
        );
        assert_eq!(outcome, AutoOutcome::Ongoing);
        assert_eq!(e.to_move(), Color::Black);
        // same UCI again is illegal (wrong turn)
        assert_eq!(e.play_uci("e2e4"), Err(RuleError::IllegalMove));
        // pawn-on-e4-style UCI parses but no piece moves that way → illegal
        assert_eq!(e.play_uci("e4e5"), Err(RuleError::IllegalMove));
        // garbage
        assert_eq!(e.play_uci("notamove"), Err(RuleError::BadUci));
    }

    #[test]
    fn scholars_mate_is_checkmate_for_white() {
        let mut e = RulesEngine::starting();
        let mut last = AutoOutcome::Ongoing;
        for uci in ["e2e4", "e7e5", "d1h5", "b8c6", "f1c4", "g8f6", "h5f7"] {
            last = e.play_uci(uci).unwrap().1;
        }
        assert_eq!(last, AutoOutcome::WhiteWin(Termination::Checkmate));
    }

    #[test]
    fn fools_mate_is_win_for_black() {
        // 1.f3 e5 2.g4 Qh4# — the classic: f2/g2 gone, d1/e2/f1 own pieces,
        // f2 square attacked → mate.
        let mut e = RulesEngine::starting();
        let mut last = AutoOutcome::Ongoing;
        for uci in ["f2f3", "e7e5", "g2g4", "d8h4"] {
            last = e.play_uci(uci).unwrap().1;
        }
        assert_eq!(last, AutoOutcome::BlackWin(Termination::Checkmate));
    }

    #[test]
    fn castling_kingside_moves_rook() {
        let mut e = RulesEngine::starting();
        for uci in ["e2e4", "e7e5", "g1f3", "b8c6", "f1c4", "f8c5", "e1g1"] {
            e.play_uci(uci).unwrap();
        }
        assert!(e.fen().contains("RNBQ1RK1"), "castled FEN: {}", e.fen());
    }

    #[test]
    fn en_passant_and_promotion_work() {
        // En passant: e4 d5 e5 f5 exf6
        let mut e = RulesEngine::starting();
        for uci in ["e2e4", "d7d5", "e4e5", "f7f5", "e5f6"] {
            e.play_uci(uci).unwrap();
        }
        assert!(
            e.fen()
                .starts_with("rnbqkbnr/ppp1p1pp/5P2/3p4/8/8/PPPP1PPP/RNBQKBNR"),
            "ep-captured FEN: {}",
            e.fen()
        );

        // Promotion to queen + check annotation
        let mut p = engine("8/P6k/8/8/8/8/7K/8 w - - 0 1");
        let (mv, _) = p.play_uci("a7a8q").unwrap();
        assert_eq!(mv.san, "a8=Q");
        // underpromotion is legal
        let mut p2 = engine("8/P6k/8/8/8/8/7K/8 w - - 0 1");
        p2.play_uci("a7a8n").unwrap();
        assert!(p2.fen().contains("N"));
    }

    #[test]
    fn stalemated_position_has_no_moves() {
        // Black king h8, white Qg6 + Kg1: every black king move is attacked.
        let s = engine("7k/8/6Q1/8/8/8/8/6K1 b - - 0 1");
        assert!(s.legal_ucis().is_empty(), "stalemated side has no moves");
    }

    #[test]
    fn stalemate_after_move_is_auto_draw() {
        // One ply earlier: white queen on g5; Qg5-g6 stalemates black.
        let mut e = engine("7k/8/8/6Q1/8/8/8/6K1 w - - 0 1");
        let (_, outcome) = e.play_uci("g5g6").unwrap();
        assert_eq!(outcome, AutoOutcome::Draw(Termination::Stalemate));
    }

    #[test]
    fn threefold_repetition_auto_claim() {
        let mut e = RulesEngine::starting();
        // Both knights out and back twice: start key reaches its 3rd occurrence.
        let mut last = AutoOutcome::Ongoing;
        for uci in [
            "g1f3", "g8f6", "f3g1", "f6g8", "g1f3", "g8f6", "f3g1", "f6g8",
        ] {
            last = e.play_uci(uci).unwrap().1;
        }
        assert_eq!(e.repetition_count(), 3);
        assert_eq!(last, AutoOutcome::Draw(Termination::Threefold));
    }

    #[test]
    fn fifty_move_auto_draw() {
        // Halfmove clock 99; a rook move (no capture, no pawn move) → 100.
        let mut e = engine("8/8/4k3/8/8/4K3/4R3/8 w - - 99 80");
        let (_, outcome) = e.play_uci("e2a2").unwrap();
        assert_eq!(outcome, AutoOutcome::Draw(Termination::FiftyMove));
    }

    #[test]
    fn insufficient_material_dead_positions() {
        assert!(engine("8/8/4k3/8/8/4K3/8/8 w - - 0 1").is_insufficient_material());
        assert!(engine("8/8/4k3/8/8/4K3/8/4N3 w - - 0 1").is_insufficient_material());
        assert!(engine("8/8/4k3/8/8/4K3/8/4B3 w - - 0 1").is_insufficient_material());
        // Same-colored bishops (a1 dark, b2 dark) — dead.
        assert!(engine("8/8/4k3/8/8/4K3/1B6/B7 w - - 0 1").is_insufficient_material());
        // Opposite minor colors is NOT dead.
        assert!(!engine("8/8/4k3/8/8/4K3/8/4BN2 w - - 0 1").is_insufficient_material());
        // Any pawn is not dead.
        assert!(!engine("8/8/4k3/8/8/4K3/4P3/8 w - - 0 1").is_insufficient_material());
    }

    #[test]
    fn flag_mating_material_approximation() {
        let board_of = |fen: &str| engine(fen).board().clone();

        // Opponent (white) has a rook → can mate → flagged black loses.
        let rook = board_of("8/8/8/8/8/4k3/8/4K2R w - - 0 1");
        assert!(RulesEngine::can_potentially_mate(
            &rook,
            Color::White,
            Color::Black
        ));

        // K+B vs bare K → no mate possible → draw.
        let bishop = board_of("8/8/8/8/8/2B1k3/8/4K3 w - - 0 1");
        assert!(!RulesEngine::can_potentially_mate(
            &bishop,
            Color::White,
            Color::Black
        ));

        // K+N vs K with a helpmate pawn on the board → can mate.
        let helpmate = board_of("8/8/8/8/8/p2Nk3/8/4K3 w - - 0 1");
        assert!(RulesEngine::can_potentially_mate(
            &helpmate,
            Color::White,
            Color::Black
        ));

        // K+N vs bare K → cannot mate → draw.
        let bare = board_of("8/8/8/8/8/3Nk3/8/4K3 w - - 0 1");
        assert!(!RulesEngine::can_potentially_mate(
            &bare,
            Color::White,
            Color::Black
        ));

        // Two knights → can mate.
        let two_knights = board_of("8/8/8/8/8/p1N1k3/8/4K1N1 w - - 0 1");
        assert!(RulesEngine::can_potentially_mate(
            &two_knights,
            Color::White,
            Color::Black
        ));
    }

    #[test]
    fn engine_rebuilds_from_fen_with_keys_for_recovery() {
        let mut e = RulesEngine::starting();
        e.play_uci("e2e4").unwrap();
        e.play_uci("e7e5").unwrap();
        let rebuilt =
            RulesEngine::from_fen_with_keys(&e.fen(), vec![STARTING_FEN.to_string()]).unwrap();
        assert_eq!(rebuilt.to_move(), Color::White);
        assert_eq!(rebuilt.ply(), 0); // ply counter restarts; DB owns the count
    }

    /// Move-generation correctness gates (shakmaty::perft vs published counts).
    #[test]
    fn perft_starting_position_d4() {
        let e = RulesEngine::starting();
        assert_eq!(shakmaty::perft(&e.pos, 4), 197_281);
    }

    #[test]
    fn perft_kiwipete_d3() {
        // The famous "Kiwipete" position — castling, ep, promotions, pins.
        let e = engine("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1");
        assert_eq!(shakmaty::perft(&e.pos, 3), 97_862);
    }

    #[test]
    fn perft_en_passant_edge_case_d5() {
        // Position 3 from the CPW perft suite (ep pin subtleties).
        let e = engine("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1");
        assert_eq!(shakmaty::perft(&e.pos, 5), 674_624);
    }
}
