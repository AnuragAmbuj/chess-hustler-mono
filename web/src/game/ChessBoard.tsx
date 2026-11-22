import React, { useState } from 'react';
import { ChessEngine, ChessMove, PieceType } from './chessEngine';
import { ChessPieceIcon } from './pieces';

interface ChessBoardProps {
  engine: ChessEngine;
  playerColor: 'white' | 'black';
  isMyTurn: boolean;
  lastMove?: { from: string; to: string } | null;
  onMakeMove: (uci: string) => void;
  disabled?: boolean;
  boardTheme?: string;
}

export function ChessBoard({
  engine,
  playerColor,
  isMyTurn,
  lastMove,
  onMakeMove,
  disabled = false,
  boardTheme,
}: ChessBoardProps) {
  const [selectedSquare, setSelectedSquare] = useState<string | null>(null);
  const [legalTargets, setLegalTargets] = useState<ChessMove[]>([]);
  const [pendingPromotion, setPendingPromotion] = useState<{ from: string; to: string } | null>(null);

  const isFlipped = playerColor === 'black';

  // Check state for the active player's king
  const inCheck = engine.isCheck();
  const checkingKingSquare = inCheck
    ? (() => {
        const coords = engine.findKing(engine.turn);
        return coords ? engine.coordsToSquare(coords[0], coords[1]) : null;
      })()
    : null;

  const handleSquareClick = (square: string) => {
    if (disabled || !isMyTurn) return;
    if (pendingPromotion) return;

    const piece = engine.getPiece(square);
    const myTurnColor = engine.turn;

    // Deselect if clicking already selected square
    if (selectedSquare === square) {
      setSelectedSquare(null);
      setLegalTargets([]);
      return;
    }

    // If a square was previously selected, check if this click is a valid move
    if (selectedSquare) {
      const match = legalTargets.find((m) => m.to === square);
      if (match) {
        const movingPiece = engine.getPiece(selectedSquare);
        const isPromotion =
          movingPiece?.type === 'p' && (square.endsWith('8') || square.endsWith('1'));

        if (isPromotion) {
          setPendingPromotion({ from: selectedSquare, to: square });
          return;
        }

        onMakeMove(match.uci);
        setSelectedSquare(null);
        setLegalTargets([]);
        return;
      }
    }

    // Select new piece if it belongs to active turn
    if (piece && piece.color === myTurnColor) {
      setSelectedSquare(square);
      const moves = engine.getLegalMoves(square);
      setLegalTargets(moves);
    } else {
      setSelectedSquare(null);
      setLegalTargets([]);
    }
  };

  const handlePromotionChoice = (promo: PieceType) => {
    if (pendingPromotion) {
      const uci = `${pendingPromotion.from}${pendingPromotion.to}${promo}`;
      onMakeMove(uci);
      setPendingPromotion(null);
      setSelectedSquare(null);
      setLegalTargets([]);
    }
  };

  // Drag & drop handlers
  const handleDragStart = (e: React.DragEvent, square: string) => {
    if (disabled || !isMyTurn) {
      e.preventDefault();
      return;
    }
    const piece = engine.getPiece(square);
    if (!piece || piece.color !== engine.turn) {
      e.preventDefault();
      return;
    }
    setSelectedSquare(square);
    setLegalTargets(engine.getLegalMoves(square));
    e.dataTransfer.setData('text/plain', square);
    e.dataTransfer.effectAllowed = 'move';
  };

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
  };

  const handleDrop = (e: React.DragEvent, targetSquare: string) => {
    e.preventDefault();
    const fromSquare = e.dataTransfer.getData('text/plain');
    if (!fromSquare || fromSquare === targetSquare) return;

    const moves = engine.getLegalMoves(fromSquare);
    const match = moves.find((m) => m.to === targetSquare);

    if (match) {
      const movingPiece = engine.getPiece(fromSquare);
      const isPromotion =
        movingPiece?.type === 'p' && (targetSquare.endsWith('8') || targetSquare.endsWith('1'));

      if (isPromotion) {
        setPendingPromotion({ from: fromSquare, to: targetSquare });
        return;
      }

      onMakeMove(match.uci);
      setSelectedSquare(null);
      setLegalTargets([]);
    }
  };

  const ranks = isFlipped ? [1, 2, 3, 4, 5, 6, 7, 8] : [8, 7, 6, 5, 4, 3, 2, 1];
  const files = isFlipped ? ['h', 'g', 'f', 'e', 'd', 'c', 'b', 'a'] : ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'];

  const themeClass = boardTheme ? `board-theme-${boardTheme.replace(/^board_/, '')}` : '';

  return (
    <div className={`chess-board-frame ${themeClass}`}>
      <div className="chess-board-grid">
        {ranks.map((rank, rIdx) =>
          files.map((file, fIdx) => {
            const square = `${file}${rank}`;
            const fileNum = square.charCodeAt(0) - 97;
            const rankNum = parseInt(square[1]!, 10) - 1;
            const isLight = (fileNum + rankNum) % 2 !== 0;

            const piece = engine.getPiece(square);
            const isSelected = selectedSquare === square;
            const isLastMove = lastMove && (lastMove.from === square || lastMove.to === square);
            const isTarget = legalTargets.some((m) => m.to === square);
            const isCheckKing = checkingKingSquare === square;

            let squareClasses = `square ${isLight ? 'square-light' : 'square-dark'}`;
            if (isSelected) squareClasses += ' square-selected';
            if (isLastMove) squareClasses += ' square-last-move';
            if (isCheckKing) squareClasses += ' square-in-check';

            return (
              <div
                key={square}
                className={squareClasses}
                onClick={() => handleSquareClick(square)}
                onDragOver={handleDragOver}
                onDrop={(e) => handleDrop(e, square)}
                role="button"
                tabIndex={-1}
                aria-label={`Square ${square}${piece ? ` ${piece.color === 'w' ? 'white' : 'black'} ${piece.type}` : ''}`}
              >
                {/* Board Coordinates */}
                {fIdx === 0 && (
                  <span className="coord coord-rank tabular-nums" aria-hidden="true">
                    {rank}
                  </span>
                )}
                {rIdx === 7 && (
                  <span className="coord coord-file" aria-hidden="true">
                    {file}
                  </span>
                )}

                {/* Piece */}
                {piece && (
                  <div
                    className="chess-piece"
                    draggable={isMyTurn && piece.color === engine.turn}
                    onDragStart={(e) => handleDragStart(e, square)}
                  >
                    <ChessPieceIcon
                      piece={piece.color === 'w' ? piece.type.toUpperCase() : piece.type.toLowerCase()}
                    />
                  </div>
                )}

                {/* Legal Move Indicators */}
                {isTarget && (
                  <div
                    className={`move-target-marker ${
                      piece ? 'marker-capture' : 'marker-empty'
                    }`}
                  />
                )}
              </div>
            );
          })
        )}
      </div>

      {/* Promotion Choice Dialog Overlay */}
      {pendingPromotion && (
        <div className="promotion-overlay">
          <div className="promotion-panel">
            <div className="promotion-heading">Promote Pawn</div>
            <div className="promotion-options">
              {(['q', 'n', 'r', 'b'] as PieceType[]).map((promo) => (
                <button
                  key={promo}
                  type="button"
                  className="promotion-option-btn"
                  onClick={() => handlePromotionChoice(promo)}
                  title={`Promote to ${promo.toUpperCase()}`}
                >
                  <ChessPieceIcon
                    piece={playerColor === 'white' ? promo.toUpperCase() : promo.toLowerCase()}
                  />
                </button>
              ))}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
