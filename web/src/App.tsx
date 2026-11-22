import { useState } from 'react';
import { AuthProvider } from './auth/useAuth';
import { AuthModal } from './auth/AuthModal';
import { PieceThemeProvider } from './game/pieces';
import { I18nProvider } from './i18n/useTranslation';
import { LobbyView } from './lobby/LobbyView';
import { GameView } from './game/GameView';
import { GameSnapshot, UserLoadout } from './types';

export function App() {
  const [activeGame, setActiveGame] = useState<{
    snapshot: GameSnapshot;
    myColor: 'white' | 'black';
    loadout?: UserLoadout;
  } | null>(null);

  const handleStartGame = (
    snapshot: GameSnapshot,
    myColor: 'white' | 'black',
    loadout?: UserLoadout
  ) => {
    setActiveGame({ snapshot, myColor, loadout });
  };

  const handleReturnToLobby = () => {
    setActiveGame(null);
  };

  return (
    <I18nProvider>
      <AuthProvider>
        <PieceThemeProvider>
          <div className="app-root">
            {activeGame ? (
              <GameView
                initialSnapshot={activeGame.snapshot}
                myColor={activeGame.myColor}
                loadout={activeGame.loadout}
                onReturnToLobby={handleReturnToLobby}
              />
            ) : (
              <LobbyView onStartGame={handleStartGame} />
            )}
            <AuthModal />
          </div>
        </PieceThemeProvider>
      </AuthProvider>
    </I18nProvider>
  );
}
