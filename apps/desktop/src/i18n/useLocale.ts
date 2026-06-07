import { useCallback, useMemo, useState } from "react";
import { defaultLocale, formatMessage, isLocale, type Locale, type MessageParams } from "./messages";

export const localeStorageKey = "skills-manage.locale";

export function readStoredLocale(storage: Storage = window.localStorage): Locale {
  const storedLocale = storage.getItem(localeStorageKey);
  return isLocale(storedLocale) ? storedLocale : defaultLocale;
}

export function writeStoredLocale(locale: Locale, storage: Storage = window.localStorage) {
  storage.setItem(localeStorageKey, locale);
}

export function useLocale() {
  const [locale, setLocaleState] = useState<Locale>(() => readStoredLocale());

  const setLocale = useCallback((nextLocale: Locale) => {
    setLocaleState(nextLocale);
    writeStoredLocale(nextLocale);
  }, []);

  const t = useCallback(
    (key: string, params?: MessageParams) => formatMessage(locale, key, params),
    [locale],
  );

  return useMemo(
    () => ({
      locale,
      setLocale,
      t,
    }),
    [locale, setLocale, t],
  );
}
