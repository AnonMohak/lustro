import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./en.json";

// All UI text lives in the JSON files in this folder. Add a language by adding a file here.
i18n.use(initReactI18next).init({
  resources: { en: { translation: en } },
  lng: "en",
  fallbackLng: "en",
  interpolation: { escapeValue: false },
});

export default i18n;
