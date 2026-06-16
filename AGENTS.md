# Verdant Desktop — Agent Guide

## Dev commands
| Command | Purpose |
|---|---|
| `npm run dev` | Vite dev server (port 5173) |
| `npm run build` | Vite build to `dist/` |
| `npm run tauri build` | Full Tauri build (Rust + frontend) |
| `npm run tauri dev` | Tauri dev with hot-reload |
| `npm run icons` | Regenerate app icons via `scripts/generate-icons.js` |

No test, lint, or typecheck commands exist. The repo has no test files.

## Setup
- **Env**: copy `.env.example` → `.env`, set `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET`
- **Linux build deps**: `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev patchelf`
- **Rust**: minimum 1.77.2, edition 2021
- **Node**: 20 (used in CI)

## Architecture
- **Rust backend** (`src-tauri/src/`) — Tauri v2 app, SQLite via rusqlite (bundled)
- **Vanilla JS frontend** (`src/`) — no framework, modules loaded directly
- **Commands bridge**: `api.js` wraps `invoke()` for every Tauri command. Add new commands to `lib.rs` `generate_handler![]` + `commands/mod.rs` + `api.js`
- **Two HTML entry points**: `index.html` (light) and `index-dark.html` (dark). Theme routed by `localStorage.verdant.appPrefs` in `src/main.js`

## Rust structure (`src-tauri/src/`)
| Module | Role |
|---|---|
| `lib.rs` | App bootstrap, plugin registration, handler list, tray icon |
| `commands/` | All `#[tauri::command]` fns — `auth`, `mail`, `compose`, `accounts`, `updater`, `app_config`, `attachments`, `shell` |
| `db.rs` | SQLite schema, Account/Email/StoredToken structs, query fns |
| `state.rs` | `DbState` shared state — conn, tokens cache, active_account_id, sync_handles |
| `auth.rs` | Gmail OAuth2 flow (tiny_http callback server) |
| `gmail.rs` / `imap_sync.rs` | Mail sync backends |
| `background_sync.rs` | Periodic sync loops (45s Gmail, 12s IMAP) |
| `smtp_send.rs` | Outgoing mail via lettre |

## Frontend structure (`src/`)
| Path | Role |
|---|---|
| `main.js` | Entrypoint — theme routing, DOMContentLoaded init, event bindings |
| `api.js` | All `invoke()` wrappers (1:1 with Tauri commands) |
| `ui/` | Shell, sidebar, reading pane, compose, settings, onboarding, thread list |
| `lib/` | `i18n.js`, `sync.js`, `hotkeys.js`, `format.js`, `contacts.js`, `toast.js` |

## Key subsystems
- **DB**: single `emails.db` in app data dir. Tables: `accounts`, `emails`, `contacts`. Schema at `db.rs:73-110`
- **Updater**: custom (not `tauri-plugin-updater`). Fetches from GitHub releases. CLI mode via `--update` flag. Supports `stable` and `nightly` channels. Downloads to `~/Downloads/`
- **App config**: JSON at `<app_data_dir>/app-config.json` — `run_in_background` (bool), `update_channel` ("stable"/"nightly")
- **Tray icon**: system tray with Show/Quit menu, click-to-focus. Close button hides to tray when `run_in_background` is true
- **Single instance**: enforced via `tauri-plugin-single-instance`
- **Capabilities**: defined in `src-tauri/capabilities/default.json` — window ops, store, process, shell:allow-open, notification, autostart

## CI
- **Nightly**: `.github/workflows/nightly-main.yml` — builds Linux (deb/rpm/appimage/pacman) + Windows (nsis) on push to `main` touching `src-tauri/`, `src/`, `index.html`, `package.json`, or `package-lock.json`. Publishes prerelease with tag `nightly-v{version}-{run_number}`
- **Stable promotion**: `.github/workflows/promote-stable.yml` — manual workflow_dispatch, copies assets from a nightly tag
- **Secrets required**: `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`

## Misc
- Window: 1200x900, no decorations (`decorations: false`), CSP disabled (`csp: null`)
- Rust linker: `target-feature=-crt-static` on Linux (in `.cargo/config.toml`)
- i18n: custom key-value system in `src/lib/i18n.js` (not a framework)
- Icons: `scripts/generate-icons.js` using jimp + sharp
- Hotkeys: configurable via `src/lib/hotkeys.js`, keybindings: compose, close, refresh, search, settings, switch account
