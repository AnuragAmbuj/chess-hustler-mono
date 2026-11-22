import { createContext, useContext, useState, ReactNode } from 'react';

export type PieceTheme = 'merida' | 'staunty' | 'cburnett';

interface PieceThemeContextType {
  pieceTheme: PieceTheme;
  setPieceTheme: (theme: PieceTheme) => void;
}

const PieceThemeContext = createContext<PieceThemeContextType>({
  pieceTheme: 'merida',
  setPieceTheme: () => {},
});

export function PieceThemeProvider({ children }: { children: ReactNode }) {
  const [pieceTheme, setPieceThemeState] = useState<PieceTheme>(() => {
    if (typeof window !== 'undefined') {
      const saved = localStorage.getItem('ch_piece_theme') as PieceTheme;
      if (saved === 'merida' || saved === 'staunty' || saved === 'cburnett') {
        return saved;
      }
    }
    return 'merida';
  });

  const setPieceTheme = (theme: PieceTheme) => {
    setPieceThemeState(theme);
    if (typeof window !== 'undefined') {
      localStorage.setItem('ch_piece_theme', theme);
    }
  };

  return (
    <PieceThemeContext.Provider value={{ pieceTheme, setPieceTheme }}>
      {children}
    </PieceThemeContext.Provider>
  );
}

export function usePieceTheme() {
  return useContext(PieceThemeContext);
}

interface PieceProps {
  piece: string; // 'P','N','B','R','Q','K' (white) or 'p','n','b','r','q','k' (black)
  className?: string;
  theme?: PieceTheme;
}

export function ChessPieceIcon({ piece, className = 'chess-piece-img', theme }: PieceProps) {
  const { pieceTheme: contextTheme } = usePieceTheme();
  const activeTheme = theme || contextTheme || 'merida';

  const isWhite = piece === piece.toUpperCase();
  const type = piece.toUpperCase();
  const prefix = isWhite ? 'w' : 'b';
  const filename = `${prefix}${type}`;

  return (
    <img
      src={`/pieces/${activeTheme}/${filename}.svg`}
      alt={`${isWhite ? 'White' : 'Black'} ${type}`}
      className={className}
      draggable={false}
      loading="eager"
      decoding="async"
    />
  );
}
