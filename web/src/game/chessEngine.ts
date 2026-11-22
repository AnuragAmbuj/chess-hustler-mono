export type PieceType = 'p' | 'n' | 'b' | 'r' | 'q' | 'k';
export type PieceColor = 'w' | 'b';

export interface Piece {
  type: PieceType;
  color: PieceColor;
}

export interface ChessMove {
  from: string; // e.g. "e2"
  to: string;   // e.g. "e4"
  promotion?: PieceType; // 'q', 'r', 'b', 'n'
  san?: string;
  uci: string;
}

export const INITIAL_FEN = 'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1';

export class ChessEngine {
  board: (Piece | null)[][] = Array(8).fill(null).map(() => Array(8).fill(null));
  turn: PieceColor = 'w';
  castling = { K: true, Q: true, k: true, q: true };
  enPassant: [number, number] | null = null; // [row, col]
  halfmove = 0;
  fullmove = 1;

  constructor(fen: string = INITIAL_FEN) {
    this.loadFen(fen);
  }

  loadFen(fen: string) {
    const parts = fen.trim().split(/\s+/);
    const rows = (parts[0] || '').split('/');
    this.board = Array(8).fill(null).map(() => Array(8).fill(null));

    for (let r = 0; r < 8; r++) {
      let c = 0;
      const rowStr = rows[r] || '';
      for (let i = 0; i < rowStr.length; i++) {
        const char = rowStr[i]!;
        if (char >= '1' && char <= '8') {
          c += parseInt(char, 10);
        } else {
          const color: PieceColor = char === char.toUpperCase() ? 'w' : 'b';
          const type = char.toLowerCase() as PieceType;
          if (r < 8 && c < 8) {
            this.board[r]![c] = { type, color };
          }
          c++;
        }
      }
    }

    this.turn = (parts[1] === 'b' ? 'b' : 'w');
    const cast = parts[2] || '-';
    this.castling = {
      K: cast.includes('K'),
      Q: cast.includes('Q'),
      k: cast.includes('k'),
      q: cast.includes('q'),
    };

    const ep = parts[3];
    if (ep && ep !== '-') {
      this.enPassant = this.squareToCoords(ep);
    } else {
      this.enPassant = null;
    }

    this.halfmove = parseInt(parts[4] || '0', 10);
    this.fullmove = parseInt(parts[5] || '1', 10);
  }

  getFen(): string {
    const rows: string[] = [];
    for (let r = 0; r < 8; r++) {
      let empty = 0;
      let rowStr = '';
      for (let c = 0; c < 8; c++) {
        const p = this.board[r]![c];
        if (!p) {
          empty++;
        } else {
          if (empty > 0) {
            rowStr += empty;
            empty = 0;
          }
          const char = p.color === 'w' ? p.type.toUpperCase() : p.type.toLowerCase();
          rowStr += char;
        }
      }
      if (empty > 0) rowStr += empty;
      rows.push(rowStr);
    }

    const castStr =
      (this.castling.K ? 'K' : '') +
      (this.castling.Q ? 'Q' : '') +
      (this.castling.k ? 'k' : '') +
      (this.castling.q ? 'q' : '') || '-';

    const epStr = this.enPassant ? this.coordsToSquare(this.enPassant[0], this.enPassant[1]) : '-';

    return `${rows.join('/')} ${this.turn} ${castStr} ${epStr} ${this.halfmove} ${this.fullmove}`;
  }

  coordsToSquare(r: number, c: number): string {
    const file = String.fromCharCode('a'.charCodeAt(0) + c);
    const rank = 8 - r;
    return `${file}${rank}`;
  }

  squareToCoords(sq: string): [number, number] {
    const col = sq.charCodeAt(0) - 'a'.charCodeAt(0);
    const row = 8 - parseInt(sq[1]!, 10);
    return [row, col];
  }

  getPiece(sq: string): Piece | null {
    const [r, c] = this.squareToCoords(sq);
    return this.board[r]?.[c] ?? null;
  }

  findKing(color: PieceColor): [number, number] | null {
    for (let r = 0; r < 8; r++) {
      for (let c = 0; c < 8; c++) {
        const p = this.board[r]![c];
        if (p && p.type === 'k' && p.color === color) {
          return [r, c];
        }
      }
    }
    return null;
  }

  isSquareAttacked(targetR: number, targetC: number, attackerColor: PieceColor): boolean {
    for (let r = 0; r < 8; r++) {
      for (let c = 0; c < 8; c++) {
        const p = this.board[r]![c];
        if (p && p.color === attackerColor) {
          if (this.canAttack(r, c, targetR, targetC, p.type, attackerColor)) {
            return true;
          }
        }
      }
    }
    return false;
  }

  private canAttack(
    fromR: number,
    fromC: number,
    toR: number,
    toC: number,
    type: PieceType,
    color: PieceColor
  ): boolean {
    const dr = toR - fromR;
    const dc = toC - fromC;

    if (type === 'p') {
      const step = color === 'w' ? -1 : 1;
      return dr === step && Math.abs(dc) === 1;
    }

    if (type === 'n') {
      return (
        (Math.abs(dr) === 1 && Math.abs(dc) === 2) ||
        (Math.abs(dr) === 2 && Math.abs(dc) === 1)
      );
    }

    if (type === 'k') {
      return Math.abs(dr) <= 1 && Math.abs(dc) <= 1;
    }

    if (type === 'b' || type === 'q') {
      if (Math.abs(dr) === Math.abs(dc) && dr !== 0) {
        const stepR = dr > 0 ? 1 : -1;
        const stepC = dc > 0 ? 1 : -1;
        let currR = fromR + stepR;
        let currC = fromC + stepC;
        while (currR !== toR || currC !== toC) {
          if (this.board[currR]![currC] !== null) return false;
          currR += stepR;
          currC += stepC;
        }
        return true;
      }
    }

    if (type === 'r' || type === 'q') {
      if ((dr === 0 && dc !== 0) || (dc === 0 && dr !== 0)) {
        const stepR = dr === 0 ? 0 : dr > 0 ? 1 : -1;
        const stepC = dc === 0 ? 0 : dc > 0 ? 1 : -1;
        let currR = fromR + stepR;
        let currC = fromC + stepC;
        while (currR !== toR || currC !== toC) {
          if (this.board[currR]![currC] !== null) return false;
          currR += stepR;
          currC += stepC;
        }
        return true;
      }
    }

    return false;
  }

  isCheck(color: PieceColor = this.turn): boolean {
    const kingPos = this.findKing(color);
    if (!kingPos) return false;
    const opponentColor: PieceColor = color === 'w' ? 'b' : 'w';
    return this.isSquareAttacked(kingPos[0], kingPos[1], opponentColor);
  }

  getLegalMoves(square?: string): ChessMove[] {
    const moves: ChessMove[] = [];
    const color = this.turn;
    const opp: PieceColor = color === 'w' ? 'b' : 'w';

    const checkFrom = square ? [this.squareToCoords(square)] : this.getAllPiecesCoords(color);

    for (const [r, c] of checkFrom) {
      const p = this.board[r]![c];
      if (!p || p.color !== color) continue;

      const fromSq = this.coordsToSquare(r, c);

      // --- Pawns ---
      if (p.type === 'p') {
        const dir = color === 'w' ? -1 : 1;
        const startRank = color === 'w' ? 6 : 1;
        const promRank = color === 'w' ? 0 : 7;

        // 1 step forward
        const fwdR = r + dir;
        if (fwdR >= 0 && fwdR < 8 && this.board[fwdR]![c] === null) {
          if (fwdR === promRank) {
            for (const promo of ['q', 'r', 'b', 'n'] as PieceType[]) {
              moves.push({ from: fromSq, to: this.coordsToSquare(fwdR, c), promotion: promo, uci: `${fromSq}${this.coordsToSquare(fwdR, c)}${promo}` });
            }
          } else {
            moves.push({ from: fromSq, to: this.coordsToSquare(fwdR, c), uci: `${fromSq}${this.coordsToSquare(fwdR, c)}` });
            // 2 steps forward
            const fwd2R = r + 2 * dir;
            if (r === startRank && this.board[fwd2R]![c] === null) {
              moves.push({ from: fromSq, to: this.coordsToSquare(fwd2R, c), uci: `${fromSq}${this.coordsToSquare(fwd2R, c)}` });
            }
          }
        }

        // Captures
        for (const dc of [-1, 1]) {
          const capC = c + dc;
          if (capC >= 0 && capC < 8 && fwdR >= 0 && fwdR < 8) {
            const target = this.board[fwdR]![capC];
            const isEnPassant = this.enPassant && this.enPassant[0] === fwdR && this.enPassant[1] === capC;
            if ((target && target.color === opp) || isEnPassant) {
              if (fwdR === promRank) {
                for (const promo of ['q', 'r', 'b', 'n'] as PieceType[]) {
                  moves.push({ from: fromSq, to: this.coordsToSquare(fwdR, capC), promotion: promo, uci: `${fromSq}${this.coordsToSquare(fwdR, capC)}${promo}` });
                }
              } else {
                moves.push({ from: fromSq, to: this.coordsToSquare(fwdR, capC), uci: `${fromSq}${this.coordsToSquare(fwdR, capC)}` });
              }
            }
          }
        }
      }

      // --- Knights ---
      if (p.type === 'n') {
        const offsets: [number, number][] = [
          [-2, -1], [-2, 1], [-1, -2], [-1, 2],
          [1, -2], [1, 2], [2, -1], [2, 1],
        ];
        for (const [dr, dc] of offsets) {
          const nr = r + dr;
          const nc = c + dc;
          if (nr >= 0 && nr < 8 && nc >= 0 && nc < 8) {
            const dest = this.board[nr]![nc];
            if (!dest || dest.color === opp) {
              moves.push({ from: fromSq, to: this.coordsToSquare(nr, nc), uci: `${fromSq}${this.coordsToSquare(nr, nc)}` });
            }
          }
        }
      }

      // --- Bishops & Queens ---
      if (p.type === 'b' || p.type === 'q') {
        const dirs: [number, number][] = [[-1, -1], [-1, 1], [1, -1], [1, 1]];
        for (const [dr, dc] of dirs) {
          let step = 1;
          while (true) {
            const nr = r + dr * step;
            const nc = c + dc * step;
            if (nr < 0 || nr >= 8 || nc < 0 || nc >= 8) break;
            const dest = this.board[nr]![nc];
            if (!dest) {
              moves.push({ from: fromSq, to: this.coordsToSquare(nr, nc), uci: `${fromSq}${this.coordsToSquare(nr, nc)}` });
            } else {
              if (dest.color === opp) {
                moves.push({ from: fromSq, to: this.coordsToSquare(nr, nc), uci: `${fromSq}${this.coordsToSquare(nr, nc)}` });
              }
              break;
            }
            step++;
          }
        }
      }

      // --- Rooks & Queens ---
      if (p.type === 'r' || p.type === 'q') {
        const dirs: [number, number][] = [[-1, 0], [1, 0], [0, -1], [0, 1]];
        for (const [dr, dc] of dirs) {
          let step = 1;
          while (true) {
            const nr = r + dr * step;
            const nc = c + dc * step;
            if (nr < 0 || nr >= 8 || nc < 0 || nc >= 8) break;
            const dest = this.board[nr]![nc];
            if (!dest) {
              moves.push({ from: fromSq, to: this.coordsToSquare(nr, nc), uci: `${fromSq}${this.coordsToSquare(nr, nc)}` });
            } else {
              if (dest.color === opp) {
                moves.push({ from: fromSq, to: this.coordsToSquare(nr, nc), uci: `${fromSq}${this.coordsToSquare(nr, nc)}` });
              }
              break;
            }
            step++;
          }
        }
      }

      // --- King ---
      if (p.type === 'k') {
        const dirs: [number, number][] = [
          [-1, -1], [-1, 0], [-1, 1],
          [0, -1],           [0, 1],
          [1, -1],  [1, 0],  [1, 1],
        ];
        for (const [dr, dc] of dirs) {
          const nr = r + dr;
          const nc = c + dc;
          if (nr >= 0 && nr < 8 && nc >= 0 && nc < 8) {
            const dest = this.board[nr]![nc];
            if (!dest || dest.color === opp) {
              moves.push({ from: fromSq, to: this.coordsToSquare(nr, nc), uci: `${fromSq}${this.coordsToSquare(nr, nc)}` });
            }
          }
        }

        // Castling
        if (!this.isCheck(color)) {
          if (color === 'w' && r === 7 && c === 4) {
            // Kingside (e1 -> g1)
            if (this.castling.K && !this.board[7]![5] && !this.board[7]![6]) {
              if (!this.isSquareAttacked(7, 5, opp) && !this.isSquareAttacked(7, 6, opp)) {
                moves.push({ from: 'e1', to: 'g1', uci: 'e1g1' });
              }
            }
            // Queenside (e1 -> c1)
            if (this.castling.Q && !this.board[7]![3] && !this.board[7]![2] && !this.board[7]![1]) {
              if (!this.isSquareAttacked(7, 3, opp) && !this.isSquareAttacked(7, 2, opp)) {
                moves.push({ from: 'e1', to: 'c1', uci: 'e1c1' });
              }
            }
          } else if (color === 'b' && r === 0 && c === 4) {
            // Kingside (e8 -> g8)
            if (this.castling.k && !this.board[0]![5] && !this.board[0]![6]) {
              if (!this.isSquareAttacked(0, 5, opp) && !this.isSquareAttacked(0, 6, opp)) {
                moves.push({ from: 'e8', to: 'g8', uci: 'e8g8' });
              }
            }
            // Queenside (e8 -> c8)
            if (this.castling.q && !this.board[0]![3] && !this.board[0]![2] && !this.board[0]![1]) {
              if (!this.isSquareAttacked(0, 3, opp) && !this.isSquareAttacked(0, 2, opp)) {
                moves.push({ from: 'e8', to: 'c8', uci: 'e8c8' });
              }
            }
          }
        }
      }
    }

    // Filter out moves that leave or put king in check
    const legal: ChessMove[] = [];
    for (const m of moves) {
      if (this.testMoveLegal(m, color)) {
        m.san = this.generateSan(m);
        legal.push(m);
      }
    }

    return legal;
  }

  private testMoveLegal(m: ChessMove, color: PieceColor): boolean {
    const [fromR, fromC] = this.squareToCoords(m.from);
    const [toR, toC] = this.squareToCoords(m.to);
    const piece = this.board[fromR]![fromC];
    if (!piece) return false;

    // Simulate move
    const captured = this.board[toR]![toC] ?? null;
    let epCaptured: Piece | null = null;
    let epCapPos: [number, number] | null = null;

    if (piece.type === 'p' && this.enPassant && toR === this.enPassant[0] && toC === this.enPassant[1]) {
      epCapPos = [fromR, toC];
      epCaptured = this.board[fromR]![toC] ?? null;
      this.board[fromR]![toC] = null;
    }

    this.board[toR]![toC] = piece;
    this.board[fromR]![fromC] = null;

    const inCheck = this.isCheck(color);

    // Revert
    this.board[fromR]![fromC] = piece;
    this.board[toR]![toC] = captured;
    if (epCapPos && epCaptured) {
      this.board[epCapPos[0]]![epCapPos[1]] = epCaptured;
    }

    return !inCheck;
  }

  applyMove(m: ChessMove | string): boolean {
    const move = typeof m === 'string' ? this.parseUci(m) : m;
    if (!move) return false;

    const [fromR, fromC] = this.squareToCoords(move.from);
    const [toR, toC] = this.squareToCoords(move.to);
    const piece = this.board[fromR]![fromC];
    if (!piece || piece.color !== this.turn) return false;

    // Handle en passant capture
    if (piece.type === 'p' && this.enPassant && toR === this.enPassant[0] && toC === this.enPassant[1]) {
      this.board[fromR]![toC] = null;
    }

    // Handle castling rook movement
    if (piece.type === 'k') {
      if (move.from === 'e1' && move.to === 'g1') {
        this.board[7]![5] = this.board[7]![7] ?? null;
        this.board[7]![7] = null;
      } else if (move.from === 'e1' && move.to === 'c1') {
        this.board[7]![3] = this.board[7]![0] ?? null;
        this.board[7]![0] = null;
      } else if (move.from === 'e8' && move.to === 'g8') {
        this.board[0]![5] = this.board[0]![7] ?? null;
        this.board[0]![7] = null;
      } else if (move.from === 'e8' && move.to === 'c8') {
        this.board[0]![3] = this.board[0]![0] ?? null;
        this.board[0]![0] = null;
      }

      if (piece.color === 'w') {
        this.castling.K = false;
        this.castling.Q = false;
      } else {
        this.castling.k = false;
        this.castling.q = false;
      }
    }

    // Castling rights loss if rook moves or is captured
    if (move.from === 'h1' || move.to === 'h1') this.castling.K = false;
    if (move.from === 'a1' || move.to === 'a1') this.castling.Q = false;
    if (move.from === 'h8' || move.to === 'h8') this.castling.k = false;
    if (move.from === 'a8' || move.to === 'a8') this.castling.q = false;

    // New en passant square if pawn double step
    if (piece.type === 'p' && Math.abs(toR - fromR) === 2) {
      this.enPassant = [(fromR + toR) / 2, fromC];
    } else {
      this.enPassant = null;
    }

    // Promotion or regular move
    if (move.promotion && piece.type === 'p') {
      this.board[toR]![toC] = { type: move.promotion, color: piece.color };
    } else {
      this.board[toR]![toC] = piece;
    }
    this.board[fromR]![fromC] = null;

    if (this.turn === 'b') {
      this.fullmove++;
    }
    this.turn = this.turn === 'w' ? 'b' : 'w';

    return true;
  }

  parseUci(uci: string): ChessMove | null {
    if (uci.length < 4) return null;
    const from = uci.slice(0, 2);
    const to = uci.slice(2, 4);
    const promotion = uci.length > 4 ? (uci[4] as PieceType) : undefined;
    return { from, to, promotion, uci };
  }

  generateSan(m: ChessMove): string {
    const [fromR, fromC] = this.squareToCoords(m.from);
    const [toR, toC] = this.squareToCoords(m.to);
    const piece = this.board[fromR]![fromC];
    if (!piece) return m.uci;

    // Castling
    if (piece.type === 'k') {
      if ((m.from === 'e1' && m.to === 'g1') || (m.from === 'e8' && m.to === 'g8')) return 'O-O';
      if ((m.from === 'e1' && m.to === 'c1') || (m.from === 'e8' && m.to === 'c8')) return 'O-O-O';
    }

    const isCapture =
      this.board[toR]![toC] !== null ||
      (piece.type === 'p' && this.enPassant && toR === this.enPassant[0] && toC === this.enPassant[1]);

    let piecePrefix = '';
    if (piece.type !== 'p') {
      piecePrefix = piece.type.toUpperCase();
    } else if (isCapture) {
      piecePrefix = m.from[0]!;
    }

    const captureChar = isCapture ? 'x' : '';
    const dest = m.to;
    const promStr = m.promotion ? `=${m.promotion.toUpperCase()}` : '';

    return `${piecePrefix}${captureChar}${dest}${promStr}`;
  }

  private getAllPiecesCoords(color: PieceColor): [number, number][] {
    const coords: [number, number][] = [];
    for (let r = 0; r < 8; r++) {
      for (let c = 0; c < 8; c++) {
        if (this.board[r]![c]?.color === color) {
          coords.push([r, c]);
        }
      }
    }
    return coords;
  }
}
