import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import { PLATFORM } from "../lib/platform";
import en from "./en.json";
import enLinux from "./en-linux.json";

type Tree = { [key: string]: string | Tree };

/** `base` with the texts of `over` in place of its own. */
export function mergeTexts(base: Tree, over: Tree): Tree {
  const out: Tree = { ...base };
  for (const [k, v] of Object.entries(over)) {
    const b = out[k];
    out[k] = typeof v === "object" && typeof b === "object" ? mergeTexts(b, v) : v;
  }
  return out;
}

// All UI text lives in the JSON files in this folder. Add a language by adding a file here.
// en-linux.json replaces the texts that name Windows features (Recycle Bin, Program Files, ...).
i18n.use(initReactI18next).init({
  resources: { en: { translation: PLATFORM === "linux" ? mergeTexts(en, enLinux) : en } },
  lng: "en",
  fallbackLng: "en",
  interpolation: { escapeValue: false },
});

export default i18n;
