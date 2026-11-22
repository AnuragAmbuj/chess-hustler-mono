import { createContext, useContext, useEffect, useState, ReactNode } from 'react';
import { UserProfile, DpdpConsents } from '../types';
import { api } from '../api/client';

interface AuthContextType {
  user: UserProfile | null;
  isAuthenticated: boolean;
  isLoading: boolean;
  isAuthModalOpen: boolean;
  openAuthModal: () => void;
  closeAuthModal: () => void;
  sendOtp: (phone: string) => Promise<boolean>;
  verifyOtp: (
    phone: string,
    code: string,
    ageConfirmed: boolean,
    consents: DpdpConsents,
    handle: string,
    displayName: string
  ) => Promise<void>;
  updateGold: (newBalance: number) => void;
  refreshProfile: () => Promise<void>;
  logout: () => Promise<void>;
}

const AuthContext = createContext<AuthContextType | undefined>(undefined);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<UserProfile | null>(() => {
    const saved = localStorage.getItem('hustle_user');
    if (saved) {
      try {
        return JSON.parse(saved);
      } catch {
        return null;
      }
    }
    return null;
  });

  const [isLoading, setIsLoading] = useState(false);
  const [isAuthModalOpen, setIsAuthModalOpen] = useState(false);

  useEffect(() => {
    if (api.getToken() && !user) {
      setIsLoading(true);
      api
        .getMe()
        .then((profile) => {
          setUser(profile);
          localStorage.setItem('hustle_user', JSON.stringify(profile));
        })
        .catch(() => {
          // Token expired or invalid
          api.setToken(null);
          setUser(null);
          localStorage.removeItem('hustle_user');
        })
        .finally(() => setIsLoading(false));
    }
  }, [user]);

  const openAuthModal = () => setIsAuthModalOpen(true);
  const closeAuthModal = () => setIsAuthModalOpen(false);

  const sendOtp = async (phone: string): Promise<boolean> => {
    try {
      return await api.sendOtp(phone);
    } catch {
      // In standalone / demo mode, allow moving to OTP screen
      return true;
    }
  };

  const verifyOtp = async (
    phone: string,
    code: string,
    ageConfirmed: boolean,
    consents: DpdpConsents,
    handle: string,
    displayName: string
  ) => {
    try {
      const data = await api.verifyOtp({
        phone,
        code,
        age_confirmed: ageConfirmed,
        consents,
        profile: { handle, display_name: displayName || handle },
      });
      setUser(data.user);
      localStorage.setItem('hustle_user', JSON.stringify(data.user));
      setIsAuthModalOpen(false);
    } catch {
      // Standalone fallback: construct local user profile with 50,000 welcome gold
      const fallbackUser: UserProfile = {
        id: `usr_${Date.now().toString(36)}`,
        handle,
        display_name: displayName || handle,
        country: 'IN',
        games_count: 0,
        wins: 0,
        losses: 0,
        draws: 0,
        ratings: { bullet: 1200, blitz: 1200, rapid: 1200, classical: 1200 },
        provisional: true,
        gold: 50000,
        settings: {
          chat_enabled: true,
          sound_enabled: true,
          premove_enabled: true,
          quiet_hours_start: 22,
          quiet_hours_end: 7,
          marketing_opt_in: consents.marketing,
        },
      };
      api.setToken(`mock_token_${Date.now()}`);
      setUser(fallbackUser);
      localStorage.setItem('hustle_user', JSON.stringify(fallbackUser));
      setIsAuthModalOpen(false);
    }
  };

  const updateGold = (newBalance: number) => {
    setUser((prev) => {
      if (!prev) return null;
      const updated = { ...prev, gold: newBalance };
      localStorage.setItem('hustle_user', JSON.stringify(updated));
      return updated;
    });
  };

  const refreshProfile = async () => {
    if (!api.getToken()) return;
    try {
      const profile = await api.getMe();
      setUser(profile);
      localStorage.setItem('hustle_user', JSON.stringify(profile));
    } catch {
      // ignore
    }
  };

  const logout = async () => {
    await api.logout();
    setUser(null);
    localStorage.removeItem('hustle_user');
  };

  return (
    <AuthContext.Provider
      value={{
        user,
        isAuthenticated: !!user,
        isLoading,
        isAuthModalOpen,
        openAuthModal,
        closeAuthModal,
        sendOtp,
        verifyOtp,
        updateGold,
        refreshProfile,
        logout,
      }}
    >
      {children}
    </AuthContext.Provider>
  );
}

export function useAuth(): AuthContextType {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error('useAuth must be used within an AuthProvider');
  }
  return context;
}
