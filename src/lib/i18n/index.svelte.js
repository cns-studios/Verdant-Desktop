import en from "./en.js";
import de from "./de.js";
import { updateAppConfig } from "../api.js";
import { readText, writeText } from "../storage.js";

const LANG_KEY = "verdant.language";
const translations = { en, de };

export const LANGUAGES = [
  { code: "en", label: "English", locale: "en-US" },
  { code: "de", label: "Deutsch", locale: "de-DE" },
];

function initialLang() {
  const saved = readText(LANG_KEY);
  if (saved && translations[saved]) return saved;
  const browser = (navigator.language || "en").split("-")[0].toLowerCase();
  return translations[browser] ? browser : "en";
}

const state = $state({ lang: initialLang() });

export function getLang() {
  return state.lang;
}

export function getLocale() {
  return LANGUAGES.find((l) => l.code === state.lang)?.locale ?? "en-US";
}

export function setLang(lang) {
  if (!translations[lang]) return;
  state.lang = lang;
  writeText(LANG_KEY, lang);
  updateAppConfig({ language: lang }).catch(() => {});
}

export function t(key, vars = {}) {
  let str = translations[state.lang][key] ?? en[key] ?? key;
  for (const [name, value] of Object.entries(vars)) {
    str = str.replaceAll(`{${name}}`, String(value));
  }
  return str;
}
