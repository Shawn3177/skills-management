import { localeLabels, locales, type Locale } from "../i18n/messages";

export function LanguageSwitch({
  label = "Language",
  locale,
  onLocaleChange,
}: {
  label?: string;
  locale: Locale;
  onLocaleChange: (locale: Locale) => void;
}) {
  return (
    <div className="language-switch" aria-label={label}>
      {locales.map((item) => (
        <button
          aria-pressed={locale === item}
          className={locale === item ? "active" : ""}
          key={item}
          onClick={() => onLocaleChange(item)}
          type="button"
        >
          {localeLabels[item]}
        </button>
      ))}
    </div>
  );
}
