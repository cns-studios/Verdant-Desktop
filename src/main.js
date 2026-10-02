import { mount } from "svelte";
import App from "./App.svelte";
import "./styles/base.css";
import "./styles/titlebar.css";
import "./styles/sidebar.css";
import "./styles/list.css";
import "./styles/reading.css";
import "./styles/compose.css";
import "./styles/settings.css";
import "./styles/onboarding.css";
import "./styles/smart-inbox.css";
import "./styles/code-card.css";
import "./styles/accounts.css";
import "./styles/updates.css";
import "./styles/whats-new.css";
import "./styles/context-menu.css";

mount(App, { target: document.getElementById("root") });
