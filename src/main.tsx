import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { FluentProvider } from "@fluentui/react-components";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import App from "./App";
import Osd from "./Osd";
import { setStatesTheme } from "./states";
import { dark, light } from "./theme";
import "./base.css";
import "./states.css";
import "./shadcn.css";

function useSystemTheme() {
  const query = window.matchMedia("(prefers-color-scheme: dark)");
  const [isDark, setDark] = useState(query.matches);
  useEffect(() => {
    const f = (e: MediaQueryListEvent) => setDark(e.matches);
    query.addEventListener("change", f);
    return () => query.removeEventListener("change", f);
  }, []);
  return isDark;
}

function Root() {
  const isDark = useSystemTheme();
  const isOsd = getCurrentWebviewWindow().label === "osd";
  document.documentElement.dataset.theme = isDark ? "dark" : "light";
  // shadcn parts read their dark tokens from a class on the root.
  document.documentElement.classList.toggle("dark", isDark);
  setStatesTheme(isDark);
  return (
    <FluentProvider theme={isDark ? dark : light} className="provider">
      {isOsd ? <Osd /> : <App />}
    </FluentProvider>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);


