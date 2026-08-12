# OpenSpec: webui-architecture

Status: Approved
Scope: `apps/webui/`, the embed site in `crates/opendoc-mcp/src/lib.rs`, and the `make` targets that build and install the WebUI.
Audience: AI agents and contributors modifying the WebUI or its embedding.

---

## Purpose

OpenDocuments ships as a single native binary. The WebUI is a React 19 + Vite 6 SPA compiled to static assets and embedded into the `opendoc` binary via `rust-embed`. The Axum server serves it from memory at runtime; there is no separate web server and no Node.js runtime in production.

This spec fixes the contract between the frontend, the build pipeline, and the embedding site so that future agents cannot silently break the WebUI by skipping the build step, editing the embed path, or inventing frontend-only fallbacks.

### Out of scope
- The TUI (canceled, see roadmap).
- The VitePress docs site (`docs-site/`) — separate publishing pipeline.
- The `opendoc-engine-lancedb` sidecar process — covered by `engine-sidecar-architecture`.

---

## Requirements

### Requirement: Supported WebUI workflows

#### Scenario: Fresh build from source
A contributor clones the repo, runs `make install`, and expects `~/.cargo/bin/opendoc` to serve a working WebUI at `http://localhost:<port>` without any extra step.

#### Scenario: Iterative frontend development
A contributor runs `npm run dev` in `apps/webui/` for HMR against a Rust server on `:3006`. The dev path must not require a full `cargo install` for every frontend change.

#### Scenario: Embedded production serve
The installed `opendoc` binary serves the SPA from memory. The SPA fallback routes unknown non-`/api` paths to `index.html`. The binary's mtime must be newer than the last edit to `apps/webui/src/` or the embed is stale.

#### Scenario: Contract change to a REST/SSE route
A backend handler changes a response shape. The frontend DTO in `apps/webui/src/lib/types.ts` must be updated to match, the call in `src/lib/api.ts` or `src/lib/sse.ts` must be updated to match, and a browser round-trip must verify the new shape end-to-end.

#### Scenario: Frontend-only workspace fallback
Forbidden. When `X-Workspace` is empty, the frontend must send the empty header and let `resolve_workspace_id` (server) apply the configured default. No client-side defaulting.

### Requirement: Architecture and runtime contracts

#### Layout and paths

| Concern | Path |
| --- | --- |
| Frontend source root | `apps/webui/src/` |
| Build output (embedded) | `apps/webui/dist/` |
| Package manifest | `apps/webui/package.json` |
| TS config | `apps/webui/tsconfig.json` |
| Vite config | `apps/webui/vite.config.ts` |
| Embed site in Rust | `crates/opendoc-mcp/src/lib.rs` — `#[derive(RustEmbed)] #[folder = "../../apps/webui/dist/"] struct Assets;` |
| Static handler | `static_handler` in the same file (SPA fallback to `index.html`) |

#### Build ordering

The build is order-dependent. `dist/` must exist before `cargo install` runs, and the only sanctioned way to enforce this is through the `Makefile`.

```
apps/webui/dist/  ── rust-embed (compile-time) ──>  opendoc binary
```

**The only sanctioned paths are `make install` and `make build`.** They invoke `npm install` + `npm run build`, then immediately rebuild and install the Rust binary in the same invocation, so `rust-embed` folds the freshly-built `dist/` into the binary. The `Makefile` encodes the dependency as `install-cli: build-web`.

**Forbidden:** running `npm run build` or `npm install` by hand. Doing so leaves `dist/` newer than the installed binary, and the next `cargo install` (without a fresh `make` run) will fold a stale — often stub — `dist/` into the binary. The footgun is asymmetric: `crates/opendoc-mcp/build.rs` silently writes a "WebUI Assets Not Compiled" stub `index.html` when `dist/` is missing at compile time, and `rust-embed` then embeds the stub. The resulting binary serves a blank-and-200 page. The `Makefile` prevents this; hand-running npm + cargo does not.

`npm run typecheck` (no `dist/` side effect) and `npm run dev` (Vite HMR, no `dist/`) may be invoked by hand for development.

#### Entry points and routing

- `apps/webui/src/main.tsx` — Vite entry, mounts `<App />`.
- `App.tsx` — auth gate; on auth success renders `<Layout>` with the current page from `appStore.currentPage`. The `PAGES` map in `App.tsx` is the canonical page registry.
- `static_handler` — serves embedded assets; falls back to `index.html` for any path not under `/api`.

#### API and SSE contract

- All REST calls go through `src/lib/api.ts`, base path `/api/v1`.
- Chat streaming goes through `src/lib/sse.ts`, `POST /api/v1/chat/stream`.
- SSE event types are `chunk`, `sources`, `confidence`, `done`, `error`. The frontend `switch (eventType)` must match these names string-for-string. Any new event type requires synchronized backend + frontend changes.
- Request body field for chat is `query` (not `message` or `prompt`).
- `X-Workspace` header carries the active workspace name or UUID; an empty header is valid and resolves server-side through the configured `active_workspace` → `default_workspace` hierarchy.
- `X-Locale` and `Accept-Language` carry the active locale; the backend uses them to localize chat citations.

##### Chat knowledge contract

The canonical WebUI Chat path is `POST /api/v1/chat/stream`. A user submits a
natural-language question to search the real documents indexed in the currently
selected workspace. Chat is not a general web search, a Graphify graph query, or a
static assistant response.

| Concern | Required behavior |
| --- | --- |
| Query | Body field is the non-empty string `query`. Whitespace-only input returns HTTP 400 before retrieval or persistence. |
| Workspace | Resolve `X-Workspace` by name or UUID using `resolve_workspace_id`; retrieval, provider lookup, conversation history, messages, and query logs use the resolved workspace ID. Results from another workspace must never appear. |
| Retrieval | Search only real indexed chunks through the production retrieval backend. No match is an empty result, never a fabricated chunk or static source. |
| Answer | Ground factual claims in retrieved chunks. Claims derived from a chunk use citation markers such as `[1]`; citation numbers must map to the emitted `sources` array. |
| No match | State that the current workspace's indexed documents do not contain relevant evidence. Do not present an unsupported model answer as if it came from the knowledge base. |
| Sources | Emit the real document/chunk path, content/snippet, relevance score, chunk type, and stable document/chunk identity needed to open or trace the source. An empty result emits `sources: []`. |
| Profile | Supported values are `fast`, `balanced`, and `precise`; they control retrieval threshold and top-k. Unknown values return HTTP 400 rather than silently selecting another profile. |
| Locale | Use `X-Locale` (with `Accept-Language` as transport compatibility) for the answer, no-result text, confidence reason, and user-visible errors. |
| Conversation | A supplied `conversationId` must exist in the resolved workspace or return 404. The canonical streaming path may create a conversation when omitted and returns its ID in `done`. Retrieval may use at most the six most recent persisted messages from that same conversation and workspace. |
| Provider unavailable | Absence of an active LLM provider may return a clearly identified extractive answer built from real retrieved chunks. A configured provider or engine failure must be reported as an error; it must not be misreported as “no matching documents.” |
| Persistence | Persist the original user query, final assistant answer, emitted sources, profile, confidence, route, workspace, and measured response time. Never persist API keys. |

##### Answer quality and presentation

Chat answers must be concise, grounded, and easy to scan:

- Lead with the direct answer. The first two or three sentences must contain the useful conclusion; add detail only when the retrieved evidence requires it.
- Use short paragraphs and Markdown lists only when they improve readability. Do not add decorative headings, repeated summaries, capability lists, or generic advice that the retrieved sources do not support.
- Every factual claim derived from a source must carry its matching citation marker. Citation numbers map to the emitted `sources` array and must never be invented.
- A non-empty `sources` array must not be described as “no documents uploaded” or “no sources found.” If the retrieved chunks do not answer the question, say that the retrieved documents do not contain enough evidence.
- An empty `sources` array produces only the localized no-evidence response. The backend must not ask the LLM to answer from general knowledge.
- The UI renders the answer before supporting evidence. Inline citations remain keyboard accessible and open the existing source preview.
- Source cards are deduplicated by stable document identity, falling back to source path when necessary, sorted by relevance, and limited to three visible cards. Additional unique sources use a localized overflow summary.
- The wide confidence bar is not part of the Chat answer hierarchy. Relevance may be shown compactly on individual source cards.
- All answer, empty, loading, and error text must use the active locale through the existing i18n system.

`POST /api/v1/chat` is the non-streaming compatibility endpoint. It follows the
same workspace, retrieval, grounding, source, profile, locale, error, and
persistence rules, and returns `QueryResult` as defined in `src/lib/types.ts`.

##### SSE ordering and terminal states

The successful stream order is:

```text
sources → confidence → chunk* → done
```

- `sources` and `confidence` describe the retrieval used for the answer.
- Every `chunk` data field is one JSON string fragment; concatenating fragments yields the final assistant answer.
- `done` is emitted exactly once and is the terminal success event. It contains `queryId`, `route`, `profile`, and `conversationId`.
- `error` is emitted exactly once as the terminal failure event. No `done` follows it, and the UI leaves streaming state deterministically.
- HTTP errors that occur before an SSE stream is established use a JSON `{ "error": string }` body with an appropriate 4xx/5xx status.
- `[待討論]` Progress events such as `status` are not part of the approved protocol. Either remove them from the backend or add them to the Rust/TypeScript DTOs, frontend switch, and this event ordering in one contract change.

##### Query entry-point boundaries

| Entry point | Purpose | Relationship to WebUI Chat |
| --- | --- | --- |
| `POST /api/v1/chat/stream` | Grounded conversational answer with citations and persistence | Canonical WebUI Chat path. |
| `POST /api/v1/chat` | Non-streaming compatibility response | Same knowledge contract; not the primary WebUI path. |
| `POST /api/v1/search` | Ranked workspace-scoped retrieval hits without answer generation | Retrieval/debugging primitive; governed by `search-index-pipeline`. |
| CLI indexing/workspace commands | Select workspace and ingest documents | Prepare the corpus; not a replacement Chat transport. |
| MCP tools/resources | Let MCP clients query OpenDocuments data | Must preserve the same real-data and workspace-isolation rules, but have their own transport DTOs. |
| Graphify graph queries | Traverse code/spec graph relationships and optional OpenDocuments context | Separate graph capability. Graphify may call OpenDocuments search as a soft fallback, but WebUI Chat does not query Graphify implicitly. |

##### Implementation conformance snapshot (2026-08-11)

Confirmed implemented:

- WebUI sends `query`, profile, optional `conversationId`, workspace, and locale through `src/lib/sse.ts`.
- Both Chat handlers resolve the workspace and pass the resolved ID to workspace-aware retrieval.
- Provider lookup, conversation validation, messages, and query logs are workspace-scoped.
- Results are mapped into source and confidence DTOs; conversation history is limited to six messages.
- A real `OpenDocuments` query returned sources from `openspec/specs/workspace-management/spec.md`, while the separate `homelab` workspace contains a different corpus.

Known non-conformance:

1. `resolve_workspace_id` currently skips configured `active_workspace` when the header is empty and goes directly to `default_workspace`, contrary to `workspace-management` §2.2.
2. Chat accepts empty queries and unknown profile names; `/search` rejects empty queries but Chat does not.
3. The backend emits undocumented `status` events that the frontend silently ignores.
4. Provider stream setup is awaited before the SSE response is returned, so a slow provider can prevent the client from receiving any event or HTTP response promptly.
5. A provider stream error emits `error`, then persists a partial answer and emits `done`; this violates terminal-event semantics.
6. Retrieval errors are collapsed into an empty result by `search_workspace`, so engine failure can be shown as “no relevant documents” instead of `503 engine_unavailable`.
7. No-result instructions currently allow the LLM to “answer directly,” which can produce an answer unsupported by the indexed corpus.
8. Extractive fallback and confidence reasons are hardcoded in Traditional Chinese instead of following the request locale.
9. `documentId` is currently populated with a file path and `chunkId` is derived from path plus content length; neither is a stable persisted document/chunk identity.
10. `response_time_ms` is persisted as the constant `100`, not a measured duration.
11. The TypeScript `StreamEvent` union omits the backend's `error` event despite `sse.ts` handling it.

#### State and DTOs

- `appStore` (`zustand`) holds `currentPage`, `theme`, `effectiveTheme`, `locale`, `profile`, and the stored backend API key.
- `chatStore` (`zustand`) holds `messages`, streaming state, `conversationId`, and the conversations list.
- DTOs in `src/lib/types.ts` must mirror the Rust response structs exactly. Adding a field to a Rust response without updating the TS interface produces silent frontend breakage.
- Zustand store interfaces must stay in sync with their `create(...)` initial state; missing initializers fail typecheck.

#### i18n

- All user-facing strings go through `translate(locale, key, values?)` in `src/lib/i18n.ts`. Inlining raw copy is a lint failure.
- Default locale is `zh-TW`. Available locales: `en`, `zh-TW`, `ko`.
- Header `<select>` switches locale; the value is persisted in `localStorage['opendocuments-locale']`.

#### Theme

- Default theme is Light. Dark mode is opt-in via the header toggle and persisted in `appStore`.
- Changing the default theme requires explicit approval and a record of the decision.

### Requirement: Settings Page

#### Information Architecture
- The Settings page is accessible via the `/settings` route.
- The page is divided into sections: Appearance, Language, RAG Profile, System Status, Corpus Readiness, and BYOK LLM Configuration.
- The BYOK LLM Configuration section is the primary focus and occupies the main position (formerly the "Active Model Route Provider" summary) to avoid duplication.
- All sections are collapsible on mobile and expandable on desktop.

#### Responsive Layout
- On narrow viewports (mobile), the layout switches to a single column with full-width sections.
- On wider viewports (desktop), the layout uses up to two columns with a maximum width constraint to ensure readability.
- Interactive elements (buttons, toggles) have a minimum touch target of 44x44 pixels.

#### Light Mode
- The Settings page adheres to the fixed Light Mode theme; the dark mode toggle is removed from the UI but the theme infrastructure remains for future restoration.
- No flash of dark mode on page load.

#### Accessible Status and Error Display
- All user-visible strings are localized via the i18n system.
- Loading states are indicated by spinners within the relevant section.
- Error states are displayed inline within the section where they occur, using semantic ARIA live regions for screen reader announcements.
- Success states (e.g., after saving) are indicated by a temporary non-intrusive banner or inline confirmation.

### Requirement: Constraints

1. **No mock data in production paths.** Stub responses, hardcoded chunks, fake hits, and static fallbacks are forbidden site-wide (`PROJECT_RULES #3317`, AGENTS.md §6 and §7). The WebUI must display whatever the real backend returns, including empty results.
2. **No private topologies.** Hardcoded private hostnames, IPs, or internal test topologies are forbidden in WebUI source. Use `127.0.0.1`, `localhost`, or RFC 5737 addresses only (AGENTS.md §6).
3. **No new top-level npm dependency** without an architectural decision record. Prefer stdlib + already-installed packages.
4. **Node.js is build-time only.** No Node.js runtime at serve time, no separate static server (`PROJECT_RULES #1445`, `#2039`).
5. **Zero-warning build.** `npm run typecheck` must be clean. The `make` chain runs `npm run build` itself; never invoke `npm run build` by hand (see §3.2).
6. **`dist/` must not be committed.** It is gitignored build output, regenerated by the `make` chain and folded into the binary at compile time.

### Requirement: Verification

#### Typecheck gate
```bash
cd apps/webui && npm run typecheck
```
Must exit 0. Any error blocks build and merge.

#### Install via `make` and mtime check
```bash
make install
ls -l ~/.cargo/bin/opendoc      # mtime newer than last edit to apps/webui/src/
curl -s http://localhost:3006/ | grep -q "assets/index-" && echo OK || echo STUB
```
`STUB` means the binary embeds the `build.rs` fallback. Re-run `make install`, restart the server, re-check. Do not proceed on a stub.

#### End-to-end (required before reporting WebUI work done)
```bash
# Foreground (debugging):
opendoc start --port 3006
# Or via the canonical systemd user unit:
systemctl --user restart opendoc-server.service
systemctl --user status  opendoc-server.service
```
- Open `http://localhost:3006` in a browser — the chat/login UI renders.
- DevTools → Network → `/api/v1/health` returns 200.
- Submit a chat query — SSE stream produces `chunk` and `sources` events, then a `done` event.
- Upload a document via the Documents page — `/api/v1/documents/upload` returns 200.

The canonical port on the development host is **3006** (per `~/.config/systemd/user/opendoc-server.service`). The Vite dev proxy (`apps/webui/vite.config.ts`) must point `/api` to `http://localhost:3006` so `npm run dev` reaches the running server.

End-to-end success is the only acceptable definition of "done" for WebUI changes (`PROJECT_RULES #3321`).

### Requirement: Open questions

- `[待討論]` Decide whether backend `status` progress events are removed or promoted into the approved SSE contract and frontend DTOs.

### References

- `PROJECT_RULES #1445` — single Rust Axum binary, no Node.js runtime.
- `PROJECT_RULES #2039` — no Docker; native single-binary deployment.
- `PROJECT_RULES #3294` — `resolve_workspace_id` server-side fallback.
- `PROJECT_RULES #3317` / `#3318` — no mocks; BYOK keys never exposed to frontend.
- `PROJECT_RULES #3321` — end-to-end verification required, not just compilation.
- `AGENTS.md` §1, §6, §7 — zero-warning compile, privacy, no-mock.
- `docs/en/webui.md`, `docs/zh-TW/webui.md` — operator-facing handbook derived from this spec.
