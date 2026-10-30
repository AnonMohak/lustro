import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import { PLATFORM, type Platform } from "../lib/platform";
import en from "./en.json";
import enLinux from "./en-linux.json";
import enMacos from "./en-macos.json";

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

/** The English texts of a platform. */
export function textsFor(platform: Platform): Tree {
  if (platform === "linux") return mergeTexts(en, enLinux);
  if (platform === "macos") return mergeTexts(mergeTexts(en, enLinux), enMacos);
  return en;
}

// All UI text lives in the JSON files in this folder. Add a language by adding a file here.
// en-linux.json replaces the texts that name Windows features (Recycle Bin, Program Files, ...).
// macOS uses these Unix texts too, and en-macos.json replaces the ones that differ there.
i18n.use(initReactI18next).init({
  resources: { en: { translation: textsFor(PLATFORM) } },
  lng: "en",
  fallbackLng: "en",
  interpolation: { escapeValue: false },
});

export default i18n;
