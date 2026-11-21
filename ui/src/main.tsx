import { render } from "preact";
import "./index.css";
import "./i18n";
import { App } from "./App";
import { applyTheme } from "./theme";

// The theme follows the system (`data-theme` on <html>), before the first paint.
applyTheme("system");

const root = document.getElementById("app");
if (root) render(<App />, root);
