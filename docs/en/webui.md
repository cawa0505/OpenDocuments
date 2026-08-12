# 🖥️ OpenDocuments WebUI — Build, Test, and Contract

**English** | [繁體中文](../zh-TW/webui.md)

This document is the authoritative source for how the embedded React WebUI is built, tested, and bound into the single-binary release. Any AI agent touching `apps/webui/` must read this first, then `AGENTS.md`, then the OpenSpec contracts cited below.

---

## 1. Where the WebUI lives

| Item | Path |
| --- | --- |
| Source root | `apps/webui/src/` |
| Build output (consumed by Rust) | `apps/webui/dist/` |
| Package manifest | `apps/webui/package.json` |
| TS config | `apps/webui/tsconfig.json` |
| Vite config | `apps/webui/vite.config.ts` |
| Embed site in Rust | `crates/opendoc-mcp/src/lib.rs` (`RustEmbed` with `#[folder = "../../apps/webui/dist/"]`) |
| Spec | [`openspec/specs/webui-architecture/spec.md`](../../openspec/specs/webui-architecture/spec.md) |

The WebUI is **not** a standalone deployment. It is compiled to static assets and embedded into the `opendoc` binary via `rust-embed`. The Axum server serves it from memory. There is no separate web server in production.

---

## 2. Stack and dependencies

- React 19 + TypeScript 5.5, Vite 6, Tailwind CSS 3.4, PostCSS, autoprefixer.
- State: `zustand` 5. Markdown rendering: `react-markdown` 9 + `rehype-highlight` 7. Icons: `lucide-react`.
- No Node.js runtime at serve time. Node.js is **only** a build-time toolchain for producing `dist/`.

Dependencies are listed in `apps/webui/package.json`. Do not add new top-level dependencies without an architectural reason; prefer stdlib + already-installed packages (see global ladder).

---

## 3. Build

### 3.1 Prerequisites
- Node.js 20+ and npm (build-time only).
- Working Rust toolchain for the embedding step.

### 3.2 The only sanctioned build path: `make`

**Never run `npm run build` or `npm install` directly.** Doing so leaves `apps/webui/dist/` newer than the installed binary, and the next `cargo install` without a fresh `make` run will embed a stale (often stub) `dist/` into the binary. The only correct path is through the `Makefile`, which orders `build-web` before `install-cli`:

```bash
# From repo root — the ONLY sanctioned paths:
make install              # build-web → install engine + main binary to ~/.cargo/bin
make build                # build-web + cargo build --release (no install)
```

`make install` / `make build` are non-negotiable. They invoke `npm install` and `npm run build` internally, then immediately rebuild and install the Rust binary so `rust-embed` folds the freshly-built `dist/` into the binary in the same invocation. Splitting the two halves by hand is how 404 stubs ship to production.

If `apps/webui/dist/` is missing or stale at `cargo install` time, `crates/opendoc-mcp/build.rs` silently writes a "WebUI Assets Not Compiled" stub `index.html` and `rust-embed` embeds that instead of the real SPA. The resulting binary serves a blank-but-200 page. The `Makefile` chain prevents this; hand-rolling npm + cargo does not.

### 3.3 Typecheck (gate, may run standalone)

```bash
cd apps/webui && npm run typecheck
```

This is the only npm script you may invoke by hand. It does not touch `dist/`. Must be clean before any `make` run.

### 3.4 Dev server (Vite HMR, no dist)

```bash
cd apps/webui && npm run dev
```

Vite listens on `http://localhost:5173` and proxies `/api` to `http://localhost:3006` (see `vite.config.ts`). Start the Rust server on `:3006` separately. This path never produces or consumes `dist/` — it is dev-only HMR against the running binary.

### 3.5 Manual cargo install (advanced, discouraged)

If you genuinely understand the ordering, the manual equivalent of `make install` is:

```bash
cd apps/webui && npm install && npm run build   # produces dist/
cd /mnt/data/btrfs-ssd/Projects/Jimmy/homelab-integration/repos/OpenDocuments
cargo install --path crates/opendoc-cli --force
cargo install --path crates/opendoc-engine-lancedb --force
```

But if `dist/` is missing at the `cargo install` step, `build.rs` writes a stub and you ship a 404 WebUI. Use `make install` unless you have a specific reason not to, and always verify the served page is the real SPA afterward (`curl http://localhost:3006/` must return HTML with `/assets/index-*.js`).

---

## 4. Verification and tests

### 4.1 Typecheck (mandatory gate)

```bash
cd apps/webui && npm run typecheck
```

Must exit 0. Any type error blocks build and merge. This is the first thing to run after touching any `.ts/.tsx` file.

### 4.2 Confirm the served WebUI is real (release gate)

After `make install` and a server restart:

```bash
curl -s http://localhost:3006/ | grep -q "assets/index-" && echo OK || echo STUB
```

If this prints `STUB`, the binary embeds the `build.rs` fallback `index.html`, not the real SPA. Rebuild via `make install`, restart the server, and re-check. Do not proceed on a stub.

### 4.3 End-to-end WebUI verification (release gate)

Compilation alone is not enough. The release gate for the WebUI is an actual browser round-trip. Minimum verifiable path:

```bash
# 1. Rebuild WebUI + reinstall binary
make install

# 2. Confirm the installed binary's mtime is newer than the source edit time
ls -l ~/.cargo/bin/opendoc

# 3. Start the unified server. Two equivalent modes:
#    a) Foreground (for interactive debugging):
opendoc start --port 3006
#    b) systemd user service (canonical on this host):
systemctl --user restart opendoc-server.service
systemctl --user status  opendoc-server.service

# 4. Open http://localhost:3006 in a browser
#    - Login page (or chat) must render, not a blank screen or 404
#    - Open DevTools → Network → confirm /api/v1/health returns 200
#    - Submit a chat query → SSE stream must produce chunks + sources
#    - Upload a document via Documents page → /api/v1/documents/upload returns 200
```

If the page is blank or assets 404, the most likely cause is a stale binary built before `dist/` was regenerated. Rebuild and reinstall, then re-check mtime.

### 4.4 HTTP contract smoke checks

Minimal curl checks for the routes the WebUI depends on:

```bash
curl -s http://localhost:3006/api/v1/health
curl -s http://localhost:3006/api/v1/workbench
curl -s -X POST http://localhost:3006/api/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{"query":"hello"}'
```

All must return 2xx with JSON bodies. 500 / 422 means a backend or contract bug, not a WebUI bug — trace the route in `crates/opendoc-mcp/src/handlers/` before touching the frontend.

---

## 5. Architecture contract (must not break)

These rules are enforced by the project specs and `AGENTS.md`. Breaking any of them is a blocker, not a style nit.

1. **Single binary.** The WebUI ships embedded via `rust-embed` inside `opendoc`. No separate static server, no Node.js runtime at serve time, no Docker (`#2039`).
2. **Embed path is `apps/webui/dist/`.** The `#[folder]` attribute in `crates/opendoc-mcp/src/lib.rs` points there. If the build output moves, update both `vite.config.ts` `outDir` and the `#[folder]` together.
3. **SPA fallback.** Unknown non-`/api` routes serve `index.html` from memory to support client-side routing. Do not remove the fallback in `static_handler`.
4. **API base is `/api/v1`.** All WebUI calls go through `src/lib/api.ts` and `src/lib/sse.ts`. The base path is hard-coded there; the Vite dev proxy repoints `/api` to `:3000` for dev only.
5. **Workspace resolution.** The WebUI sends `X-Workspace` from `localStorage['active-workspace']`. An empty header resolves server-side to the configured default workspace (`resolve_workspace_id`, `#3294`). Do not invent workspace fallbacks in the frontend.
6. **BYOK / API keys never leave the server.** The WebUI holds an API key in `localStorage` only for backend auth; LLM provider keys are stored in SQLite and never returned to the frontend (`#3318`).
7. **No mock data in production paths.** Stub responses, hardcoded chunks, and fake hits are forbidden site-wide (`#3317`, AGENTS.md §6/§7). The WebUI must display whatever the real backend returns, including empty results.
8. **i18n.** All user-facing strings go through `src/lib/i18n.ts` (`translate`). Do not inline raw copy. The header `<select>` switches `locale` in `localStorage['opendocuments-locale']`; the value is sent as `X-Locale` and `Accept-Language`.
9. **Light Mode default.** The default theme is Light; dark mode is opt-in via the header toggle and persisted in the app store. Do not change the default without approval (`fix(webui): lock default theme to clean light mode`).
10. **Zero-warning compilation** applies to the TS build as well as Rust. `npm run typecheck` must be clean before `make` runs. The `make` chain itself runs `npm run build`; never invoke it by hand.

---

## 6. Source map (where things live)

```
apps/webui/src/
├── App.tsx                  # Auth gate → Layout → page router
├── main.tsx                 # Vite entry
├── lib/
│   ├── api.ts               # All REST calls (base /api/v1)
│   ├── sse.ts               # Chat streaming (POST /api/v1/chat/stream)
│   ├── auth.ts              # Stored API key for backend auth
│   ├── i18n.ts              # translate(locale, key, values?)
│   └── types.ts             # Shared DTOs (match backend exactly)
├── stores/
│   ├── appStore.ts          # currentPage, theme, locale, profile
│   └── chatStore.ts         # messages, streaming state, conversations
└── components/
    ├── auth/                # LoginPage
    ├── layout/              # Layout, Sidebar, CommandPalette
    ├── dashboard/           # UnifiedDashboard
    ├── chat/                # ChatPage, ChatInput, ChatMessage, SourceCard
    ├── documents/           # DocumentsPage
    ├── collections/         # CollectionsPage
    ├── dictionary/          # DictionaryPage
    ├── settings/            # SettingsPage (BYOK providers)
    ├── health/              # HealthPage
    ├── connectors/          # ConnectorsPage
    ├── workspaces/          # WorkspacesPage
    ├── plugins/             # PluginsPage
    └── admin/               # Admin dashboards
```

`PAGES` in `App.tsx` is the canonical list of routable pages. Add a new page by registering it there and adding a sidebar entry.

---

## 7. Anti-patterns (what broke before)

- Stripping `ChatPage.tsx` to import-only and leaving no `export function ChatPage` — breaks the bundle. Always keep the component body intact when refactoring.
- Adding fields to a Zustand store interface (`ChatState`) but not initializing them in the `create(...)` object — TypeScript blocks the build. Keep the interface and the initial state in sync.
- Introducing `onStatus` / `phase` fields that the SSE protocol does not emit — dead branches. The SSE event types in `src/lib/sse.ts` must match the backend `event:` lines exactly (`chunk`, `sources`, `confidence`, `done`, `error`).
- Inventing workspace resolution in the frontend (e.g. picking a hardcoded workspace when `X-Workspace` is empty). Always leave the header empty and let `resolve_workspace_id` decide.
- Hardcoding private hostnames, IPs, or test topologies. Use `127.0.0.1`, `localhost`, or RFC 5737 addresses only (AGENTS.md §6).
- Editing the backend route contract without updating both `src/lib/api.ts` and `src/lib/types.ts`. The DTOs in `types.ts` must mirror the Rust response structs.

---

## 8. When something breaks

1. `cd apps/webui && npm run typecheck` — read the first error.
2. `curl -s http://localhost:3006/ | grep -q "assets/index-"` — if grep misses, the binary embeds the stub. Rebuild via `make install`, restart the server, re-check.
3. If typecheck and build are clean but the page is blank in the browser, the installed binary is stale. Rebuild `dist/` → `make install` → check `~/.cargo/bin/opendoc` mtime.
4. If the page loads but API calls 404/500, trace the route in `crates/opendoc-mcp/src/lib.rs` and `crates/opendoc-mcp/src/handlers/`. The frontend is usually correct; the backend is the suspect.
5. For SSE streaming bugs, compare the `event:` names emitted by the backend against the `switch (eventType)` in `src/lib/sse.ts`. They must match string-for-string.

---

## 9. Release checklist (WebUI slice)

- [ ] `apps/webui/src/` changes pass `npm run typecheck` with zero errors.
- [ ] `make install` succeeds and `~/.cargo/bin/opendoc` mtime is newer than the source edit.
- [ ] `curl -s http://localhost:3006/ | grep -q "assets/index-"` returns OK — not a stub.
- [ ] `opendoc start --port 3006` starts without errors (or `systemctl --user restart opendoc-server.service` brings the unit to active).
- [ ] Browser at `http://localhost:3006` renders the chat UI and `/api/v1/health` returns 200.
- [ ] At least one chat round-trip and one document upload succeed end-to-end.
- [ ] No new top-level npm dependency unless an architectural decision was recorded.
- [ ] `dist/` is **not** committed (it is gitignored build output).

---

*Last updated: 2026-08-11. Owner: OpenDocuments core. Governing specs: `single-binary-architecture`, `webui-architecture` (OpenSpec).*