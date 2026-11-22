import React, { createContext, useContext, useState, useEffect } from 'react';
import { SupportedLang, Translations, translations } from './translations';

interface I18nContextType {
  lang: SupportedLang;
  setLang: (lang: SupportedLang) => void;
  t: Translations;
}

const I18nContext = createContext<I18nContextType>({
  lang: 'en',
  setLang: () => {},
  t: translations.en,
});

export const I18nProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [lang, setLangState] = useState<SupportedLang>(() => {
    const saved = localStorage.getItem('ch_lang');
    return saved === 'hi' ? 'hi' : 'en';
  });

  const setLang = (newLang: SupportedLang) => {
    setLangState(newLang);
    localStorage.setItem('ch_lang', newLang);
  };

  useEffect(() => {
    document.documentElement.lang = lang;
  }, [lang]);

  const value: I18nContextType = {
    lang,
    setLang,
    t: translations[lang],
  };

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
};

export const useTranslation = () => useContext(I18nContext);
