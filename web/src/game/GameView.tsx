import { useState, useEffect, useMemo, useCallback } from 'react';
import { GameSnapshot, GameResultPayload, MoveRecord, UserLoadout } from '../types';
import { ChessEngine, Piece } from './chessEngine';
import { ChessBoard } from './ChessBoard';
import { ClocksManager, PlayerBar } from './Clocks';
import { MoveHistory } from './MoveHistory';
import { GameControls } from './GameControls';
import { GameChat } from './GameChat';
import { ReportModal } from './ReportModal';
import { GameResultModal } from './GameResultModal';
import { chessAudio } from './sound';
import { useRealtime } from '../realtime/useRealtime';
import { useAuth } from '../auth/useAuth';
import { api } from '../api/client';

interface GameViewProps {
  initialSnapshot: GameSnapshot;
  myColor: 'white' | 'black';
  loadout?: UserLoadout;
  onReturnToLobby: () => void;
}

export function GameView({
  initialSnapshot,
  myColor,
  loadout,
  onReturnToLobby,
}: GameViewProps) {
  const { user, updateGold } = useAuth();
  const { subscribe, subscribeChannel, unsubscribeChannel, sendMove } = useRealtime();

  const [snapshot, setSnapshot] = useState<GameSnapshot>(initialSnapshot);
  const [moves, setMoves] = useState<MoveRecord[]>(initialSnapshot.moves || []);
  const [clocks, setClocks] = useState(initialSnapshot.clocks);
  const [lastMove, setLastMove] = useState<{ from: string; to: string } | null>(null);
  const [drawOffered, setDrawOffered] = useState(false);
  const [gameResult, setGameResult] = useState<GameResultPayload | null>(null);
  const [isResultModalOpen, setIsResultModalOpen] = useState(false);
  const [sidebarTab, setSidebarTab] = useState<'moves' | 'chat'>('moves');
  const [unreadChat, setUnreadChat] = useState(false);
  const [isReportOpen, setIsReportOpen] = useState(false);

  // Initialize or update chess engine with current FEN
  const engine = useMemo(() => {
    return new ChessEngine(snapshot.fen);
  }, [snapshot.fen]);

  const activeTurnColor: 'white' | 'black' = engine.turn === 'w' ? 'white' : 'black';
  const isMyTurn = activeTurnColor === myColor && snapshot.status === 'active';

  // Calculate captured pieces & material advantage from engine board state
  const { capturedByWhite, capturedByBlack, whiteAdvantage, blackAdvantage } = useMemo(() => {
    const initialCounts: Record<'w' | 'b', Record<string, number>> = {
      w: { p: 8, n: 2, b: 2, r: 2, q: 1 },
      b: { p: 8, n: 2, b: 2, r: 2, q: 1 },
    };
    const currentCounts: Record<'w' | 'b', Record<string, number>> = {
      w: { p: 0, n: 0, b: 0, r: 0, q: 0 },
      b: { p: 0, n: 0, b: 0, r: 0, q: 0 },
    };

    for (let r = 0; r < 8; r++) {
      for (let c = 0; c < 8; c++) {
        const p = engine.board[r]?.[c] as Piece | null;
        if (p && p.type !== 'k') {
          currentCounts[p.color][p.type] = (currentCounts[p.color][p.type] || 0) + 1;
        }
      }
    }

    const glyphs: Record<string, string> = { q: '♛', r: '♜', b: '♝', n: '♞', p: '♟' };
    const values: Record<string, number> = { p: 1, n: 3, b: 3, r: 5, q: 9 };

    const capturedByW: string[] = [];
    const capturedByB: string[] = [];
    let wVal = 0;
    let bVal = 0;

    for (const t of ['q', 'r', 'b', 'n', 'p'] as const) {
      const bLost = Math.max(0, (initialCounts.b[t] || 0) - (currentCounts.b[t] || 0));
      for (let i = 0; i < bLost; i++) capturedByW.push(glyphs[t]!);

      const wLost = Math.max(0, (initialCounts.w[t] || 0) - (currentCounts.w[t] || 0));
      for (let i = 0; i < wLost; i++) capturedByB.push(glyphs[t]!);

      wVal += (currentCounts.w[t] || 0) * values[t]!;
      bVal += (currentCounts.b[t] || 0) * values[t]!;
    }

    return {
      capturedByWhite: capturedByW,
      capturedByBlack: capturedByB,
      whiteAdvantage: Math.max(0, wVal - bVal),
      blackAdvantage: Math.max(0, bVal - wVal),
    };
  }, [engine]);

  // Realtime subscription for game events
  useEffect(() => {
    const channel = `g:${snapshot.id}`;
    subscribeChannel(channel);

    const unsubMove = subscribe('game.move', (envelope) => {
      const data = envelope.data;
      if (data) {
        const from = data.uci.slice(0, 2);
        const to = data.uci.slice(2, 4);
        const isCapture = !!engine.getPiece(to);
        engine.applyMove(data.uci);
        if (engine.isCheck(engine.turn)) chessAudio.playCheck();
        else if (isCapture) chessAudio.playCapture();
        else chessAudio.playMove();
        setLastMove({ from, to });

        setMoves((prev) => {
          if (prev.some((m) => m.ply === data.ply)) return prev;
          return [
            ...prev,
            {
              ply: data.ply,
              uci: data.uci,
              san: data.san || data.uci,
              ts: data.ts,
            },
          ];
        });

        if (data.clocks) {
          setClocks({
            white_ms: data.clocks.white_ms,
            black_ms: data.clocks.black_ms,
            server_ts: data.ts || new Date().toISOString(),
          });
        }

        setSnapshot((prev) => ({
          ...prev,
          fen: data.fen || engine.getFen(),
          ply: data.ply,
          version: data.version ?? prev.version + 1,
        }));
      }
    });

    const unsubResult = subscribe('game.result', (envelope) => {
      const res = envelope.data as GameResultPayload;
      chessAudio.playGameEnd();
      setGameResult(res);
      setIsResultModalOpen(true);
      setSnapshot((prev) => ({
        ...prev,
        status: 'finished',
        result: res.result,
        termination: res.termination,
      }));

      const earned =
        myColor === 'white'
          ? res.gold?.white_earned ?? res.gold?.earned
          : res.gold?.black_earned ?? res.gold?.earned;
      if (user && earned !== undefined) {
        updateGold(user.gold + earned);
      }
    });

    const unsubAbort = subscribe('game.abort', (envelope) => {
      const abortData = envelope.data;
      const res: GameResultPayload = {
        result: 'draw',
        termination: abortData?.reason || 'aborted by player',
      };
      setGameResult(res);
      setIsResultModalOpen(true);
      setSnapshot((prev) => ({ ...prev, status: 'aborted' }));
    });

    const unsubDraw = subscribe('game.draw_offer', () => {
      setDrawOffered(true);
    });

    return () => {
      unsubMove();
      unsubResult();
      unsubAbort();
      unsubDraw();
      unsubscribeChannel(channel);
    };
  }, [snapshot.id, engine, myColor, user, subscribe, subscribeChannel, unsubscribeChannel, updateGold]);

  const handleMakeMove = useCallback(
    (uci: string) => {
      if (snapshot.status !== 'active') return;

      const from = uci.slice(0, 2);
      const to = uci.slice(2, 4);
      setLastMove({ from, to });

      const isCapture = !!engine.getPiece(to);
      const parsedMove = engine.parseUci(uci);
      const san = parsedMove ? engine.generateSan(parsedMove) : uci;
      engine.applyMove(uci);
      if (engine.isCheck(engine.turn)) chessAudio.playCheck();
      else if (isCapture) chessAudio.playCapture();
      else chessAudio.playMove();

      const nextPly = snapshot.ply + 1;
      const newFen = engine.getFen();

      setMoves((prev) => [
        ...prev,
        {
          ply: nextPly,
          uci,
          san,
          ts: new Date().toISOString(),
        },
      ]);

      setSnapshot((prev) => ({
        ...prev,
        fen: newFen,
        ply: nextPly,
      }));

      sendMove(snapshot.id, uci);

      // Check if this move ended the game
      const legal = engine.getLegalMoves();
      if (legal.length === 0) {
        const isCheck = engine.isCheck(engine.turn);
        if (isCheck) {
          const winner = engine.turn === 'w' ? 'black' : 'white';
          const finishResult: GameResultPayload = {
            result: winner,
            termination: 'checkmate',
            ratings: {
              white: { before: snapshot.white.rating, after: snapshot.white.rating + (winner === 'white' ? 16 : -16), delta: winner === 'white' ? 16 : -16 },
              black: { before: snapshot.black.rating, after: snapshot.black.rating + (winner === 'black' ? 16 : -16), delta: winner === 'black' ? 16 : -16 },
            },
            gold: {
              white_earned: winner === 'white' ? 25 : 5,
              black_earned: winner === 'black' ? 25 : 5,
            },
          };
          setGameResult(finishResult);
          setIsResultModalOpen(true);
          setSnapshot((prev) => ({ ...prev, status: 'finished', result: winner, termination: 'checkmate' }));
          return;
        } else {
          const finishResult: GameResultPayload = {
            result: 'draw',
            termination: 'stalemate',
            ratings: {
              white: { before: snapshot.white.rating, after: snapshot.white.rating, delta: 0 },
              black: { before: snapshot.black.rating, after: snapshot.black.rating, delta: 0 },
            },
            gold: {
              white_earned: 10,
              black_earned: 10,
            },
          };
          setGameResult(finishResult);
          setIsResultModalOpen(true);
          setSnapshot((prev) => ({ ...prev, status: 'finished', result: 'draw', termination: 'stalemate' }));
          return;
        }
      }

      // Offline practice mode response
      if (snapshot.id.startsWith('practice_') && engine.turn !== (myColor === 'white' ? 'w' : 'b')) {
        setTimeout(() => {
          const opponentMoves = engine.getLegalMoves();
          if (opponentMoves.length > 0) {
            const randomMove = opponentMoves[Math.floor(Math.random() * opponentMoves.length)]!;
            const isOppCapture = !!engine.getPiece(randomMove.to);
            const oppSan = engine.generateSan(randomMove);
            engine.applyMove(randomMove.uci);
            if (engine.isCheck(engine.turn)) chessAudio.playCheck();
            else if (isOppCapture) chessAudio.playCapture();
            else chessAudio.playMove();
            const oppPly = nextPly + 1;

            setLastMove({ from: randomMove.from, to: randomMove.to });
            setMoves((prev) => [
              ...prev,
              {
                ply: oppPly,
                uci: randomMove.uci,
                san: oppSan,
                ts: new Date().toISOString(),
              },
            ]);
            setSnapshot((prev) => ({
              ...prev,
              fen: engine.getFen(),
              ply: oppPly,
            }));

            // Check if bot move delivered checkmate or stalemate
            const playerLegal = engine.getLegalMoves();
            if (playerLegal.length === 0) {
              const botIsCheck = engine.isCheck(engine.turn);
              const botWon = botIsCheck ? (myColor === 'white' ? 'black' : 'white') : 'draw';
              const term = botIsCheck ? 'checkmate' : 'stalemate';
              const botFinishResult: GameResultPayload = {
                result: botWon as 'white' | 'black' | 'draw',
                termination: term,
                gold: {
                  white_earned: botWon === 'white' ? 25 : 5,
                  black_earned: botWon === 'black' ? 25 : 5,
                },
              };
              setGameResult(botFinishResult);
              setIsResultModalOpen(true);
              setSnapshot((prev) => ({ ...prev, status: 'finished', result: botWon, termination: term }));
            }
          }
        }, 500);
      }
    },
    [snapshot.status, snapshot.ply, snapshot.id, snapshot.white.rating, snapshot.black.rating, engine, sendMove, myColor]
  );

  const handleResign = async () => {
    try {
      const res = await api.resignGame(snapshot.id);
      setGameResult(res);
      setIsResultModalOpen(true);
    } catch {
      const winner = myColor === 'white' ? 'black' : 'white';
      const fallbackResult: GameResultPayload = {
        result: winner,
        termination: 'resignation',
        ratings: {
          white: { before: snapshot.white.rating, after: snapshot.white.rating + (winner === 'white' ? 16 : -16), delta: winner === 'white' ? 16 : -16 },
          black: { before: snapshot.black.rating, after: snapshot.black.rating + (winner === 'black' ? 16 : -16), delta: winner === 'black' ? 16 : -16 },
        },
        gold: {
          white_earned: winner === 'white' ? 25 : 5,
          black_earned: winner === 'black' ? 25 : 5,
        },
      };
      setGameResult(fallbackResult);
      setIsResultModalOpen(true);
    }
  };

  const handleAbort = async () => {
    try {
      await api.abortGame(snapshot.id);
    } catch {
      const fallbackResult: GameResultPayload = {
        result: 'draw',
        termination: 'game aborted',
      };
      setGameResult(fallbackResult);
      setIsResultModalOpen(true);
    }
  };

  const handleOfferDraw = async () => {
    setDrawOffered(true);
    await api.offerDraw(snapshot.id).catch(() => {});
  };

  const handleClaimTimeout = async () => {
    try {
      const res = await api.claimTimeout(snapshot.id);
      setGameResult(res);
      setIsResultModalOpen(true);
    } catch {
      handleTimeout(myColor === 'white' ? 'black' : 'white');
    }
  };

  const handleTimeout = (timedOutColor: 'white' | 'black') => {
    const winner = timedOutColor === 'white' ? 'black' : 'white';
    const timeoutResult: GameResultPayload = {
      result: winner,
      termination: 'time forfeit',
      ratings: {
        white: { before: snapshot.white.rating, after: snapshot.white.rating + (winner === 'white' ? 16 : -16), delta: winner === 'white' ? 16 : -16 },
        black: { before: snapshot.black.rating, after: snapshot.black.rating + (winner === 'black' ? 16 : -16), delta: winner === 'black' ? 16 : -16 },
      },
      gold: {
        white_earned: winner === 'white' ? 25 : 5,
        black_earned: winner === 'black' ? 25 : 5,
      },
    };
    setGameResult(timeoutResult);
    setIsResultModalOpen(true);
  };

  // Determine top and bottom players based on orientation
  const isWhite = myColor === 'white';
  const topPlayer = isWhite ? snapshot.black : snapshot.white;
  const topColor = isWhite ? 'black' : 'white';
  const bottomPlayer = isWhite ? snapshot.white : snapshot.black;
  const bottomColor = isWhite ? 'white' : 'black';

  return (
    <div className="game-screen">
      <ClocksManager
        clocks={clocks}
        activeColor={activeTurnColor}
        isPaused={snapshot.status !== 'active'}
        onTimeout={handleTimeout}
      >
        {({ whiteMs, blackMs }) => {
          const topRemaining = isWhite ? blackMs : whiteMs;
          const bottomRemaining = isWhite ? whiteMs : blackMs;
          const opponentTimedOut = topRemaining <= 0 && snapshot.status === 'active';

          return (
            <div className="game-stage">
              {/* Board and Player Panels */}
              <div className="board-column">
                {/* Opponent Player Bar (Top) */}
                <PlayerBar
                  player={topPlayer}
                  color={topColor}
                  isYou={false}
                  isActive={activeTurnColor === topColor && snapshot.status === 'active'}
                  remainingMs={topRemaining}
                  capturedPieces={isWhite ? capturedByBlack : capturedByWhite}
                  materialAdvantage={isWhite ? blackAdvantage : whiteAdvantage}
                />

                {/* Interactive Chess Board */}
                <ChessBoard
                  engine={engine}
                  playerColor={myColor}
                  isMyTurn={isMyTurn}
                  lastMove={lastMove}
                  onMakeMove={handleMakeMove}
                  disabled={snapshot.status !== 'active'}
                  boardTheme={loadout?.board_id || undefined}
                />

                {/* You Player Bar (Bottom) */}
                <PlayerBar
                  player={bottomPlayer}
                  color={bottomColor}
                  isYou={true}
                  isActive={activeTurnColor === bottomColor && snapshot.status === 'active'}
                  remainingMs={bottomRemaining}
                  capturedPieces={isWhite ? capturedByWhite : capturedByBlack}
                  materialAdvantage={isWhite ? whiteAdvantage : blackAdvantage}
                />
              </div>

              {/* Sidebar with Meta, Move History & Game Controls */}
              <aside className="game-sidebar">
                <div className="sidebar-card">
                  {/* Game Meta Header */}
                  <div className="game-meta-row">
                    <div className="meta-left">
                      <span className="pool-chip">{snapshot.pool.toUpperCase()}</span>
                      <span className="tc-chip tabular-nums">
                        {snapshot.tc.initial_s / 60}+{snapshot.tc.increment_s}
                      </span>
                      <span className="rated-chip">{snapshot.rated ? 'Rated' : 'Casual'}</span>
                    </div>
                    <div className="meta-actions">
                      <button
                        type="button"
                        className="btn btn-ghost btn-xs text-danger"
                        onClick={() => setIsReportOpen(true)}
                        title="Report Opponent"
                      >
                        🚩 Report
                      </button>
                      <button
                        type="button"
                        className="btn btn-ghost btn-xs"
                        onClick={onReturnToLobby}
                        title="Return to Lobby"
                      >
                        ← Exit
                      </button>
                    </div>
                  </div>

                  {/* Active Turn Status Pill */}
                  <div className="turn-status-banner" data-turn={activeTurnColor}>
                    <span className="turn-dot" />
                    <span className="turn-label">
                      {snapshot.status !== 'active'
                        ? 'Game Finished'
                        : isMyTurn
                        ? 'Your turn to move'
                        : `${topPlayer.handle} is thinking…`}
                    </span>
                  </div>

                  {/* Sidebar Tab Switcher (Moves vs Chat) */}
                  <div className="sidebar-tab-bar">
                    <button
                      type="button"
                      className={`sidebar-tab-btn ${sidebarTab === 'moves' ? 'active' : ''}`}
                      onClick={() => setSidebarTab('moves')}
                    >
                      Moves ({moves.length})
                    </button>
                    <button
                      type="button"
                      className={`sidebar-tab-btn ${sidebarTab === 'chat' ? 'active' : ''}`}
                      onClick={() => {
                        setSidebarTab('chat');
                        setUnreadChat(false);
                      }}
                    >
                      Chat {unreadChat && <span className="unread-dot" />}
                    </button>
                  </div>

                  {sidebarTab === 'moves' ? (
                    <MoveHistory moves={moves} currentPly={snapshot.ply} />
                  ) : (
                    <GameChat
                      gameId={snapshot.id}
                      opponentHandle={topPlayer.handle}
                      onNewMessage={() => {
                        if (sidebarTab !== 'chat') {
                          setUnreadChat(true);
                        }
                      }}
                    />
                  )}

                  {/* Actions Toolbar */}
                  <GameControls
                    plyCount={moves.length}
                    isDrawOffered={drawOffered}
                    canClaimTimeout={opponentTimedOut}
                    onResign={handleResign}
                    onAbort={handleAbort}
                    onOfferDraw={handleOfferDraw}
                    onClaimTimeout={handleClaimTimeout}
                    disabled={snapshot.status !== 'active'}
                  />
                </div>
              </aside>
            </div>
          );
        }}
      </ClocksManager>

      <GameResultModal
        isOpen={isResultModalOpen}
        result={gameResult}
        snapshot={snapshot}
        myColor={myColor}
        onReturnToLobby={onReturnToLobby}
      />

      <ReportModal
        isOpen={isReportOpen}
        onClose={() => setIsReportOpen(false)}
        targetHandle={topPlayer.handle}
        gameId={snapshot.id}
      />
    </div>
  );
}
