# Verdant Desktop

[![Nightly Build And Release](https://github.com/cns-studios/Verdant-Desktop/actions/workflows/nightly-main.yml/badge.svg)](https://github.com/cns-studios/Verdant-Desktop/actions/workflows/nightly-main.yml)

A desktop mail client built with Tauri 2, Rust and Svelte 5.

## Development

Requirements: Rust (stable), Node 22+, pnpm, and the Tauri system dependencies for your platform.

```sh
pnpm install
pnpm tauri dev
```

| Command | What it does |
| --- | --- |
| `pnpm dev` | Frontend only, on the Vite dev server |
| `pnpm build` | Production build of the frontend into `dist/` |
| `pnpm tauri dev` | The full app with hot reload |
| `pnpm tauri build` | Release bundles |
| `cargo test` (in `src-tauri/`) | Backend tests |

Google sign-in needs `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET` at build time, see `.env.example`.

## Layout

### Frontend (`src/`)

Plain Svelte 5 with Vite, no SvelteKit and no router. State lives in rune-based stores, components only render it.

| Path | Contents |
| --- | --- |
| `App.svelte` | Boot sequence: loading, onboarding or the mail shell |
| `state/` | Reactive stores: `mail`, `session`, `compose`, `selection`, `prefs`, `smartInbox`, `updates`, `ui` |
| `lib/` | Framework-free helpers: Tauri `api`, `i18n`, formatting, hotkeys, sanitizing, storage |
| `components/shell/` | Window frame, titlebar, pane resizing |
| `components/sidebar/` | Mailboxes, smart categories, account switcher |
| `components/list/` | Mail list, rows, bulk actions |
| `components/reading/` | Message and thread views, attachments, verification codes |
| `components/compose/` | Compose window, recipients, formatting, send hold |
| `components/settings/` | One component per settings tab |
| `components/onboarding/` | First-run and add-account flow |
| `components/overlays/` | Context menu, popups, update and changelog dialogs |
| `styles/` | Global stylesheets, one per feature |

### Backend (`src-tauri/src/`)

| Path | Contents |
| --- | --- |
| `commands/` | Tauri commands, grouped by feature |
| `db/` | SQLite schema, models and queries |
| `gmail/` | Gmail REST API client and sync |
| `imap_client/` | IMAP connection, sync and actions |
| `sync/` | Background sync loops and notifications |
| `smart_inbox/` | Inbox categorisation |
| `auth.rs` | Google OAuth with PKCE |
| `crypto.rs` | Credential storage in the system keyring |
| `smtp_send.rs` | Outgoing mail |

## ToDo's

- [x] CI Pipeline (GH Actions)
- [x] Other SMTP Support (Currently Gmail only)
- [ ] Performance testing
