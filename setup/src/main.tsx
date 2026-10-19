import React from "react";
import ReactDOM from "react-dom/client";
import { Setup } from "./Setup";
import "../../src/styles/app.css";
import "./setup.css";

// This is an installer, not a web page: no browser menu, no reload.
window.addEventListener("contextmenu", (e) => e.preventDefault());
window.addEventListener("keydown", (e) => {
  if (e.key === "F5" || (e.ctrlKey && e.key.toLowerCase() === "r")) e.preventDefault();
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Setup />
  </React.StrictMode>,
);
