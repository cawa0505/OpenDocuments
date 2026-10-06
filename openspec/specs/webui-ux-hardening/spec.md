# webui-ux-hardening Specification

## Purpose
Keep the OpenDocuments WebUI trustworthy under real use: bounded recovery of chat sessions (no permanent wedge), locale-complete zh-TW copy with Taiwan conventions, minimal exposure of host paths, disambiguation of same-name documents before destructive actions, and single-source state for the retrieval preference. Sourced from the 2026-09-27 dogfood audit; screen text is part of correctness, not cosmetics (AGENTS §0.1).

## Requirements

### Requirement: Chat session recovery SHALL never wedge permanently

The WebUI SHALL bound the wait for any recovering or in-flight chat session. If no meaningful activity arrives within a bounded window, the session SHALL transition to an explicit error state offering cancel and retry, and the composer SHALL be re-enabled. A permanently disabled "thinking" state with no user exit is forbidden.

#### Scenario: Recovering a prior session
- **WHEN** the user opens a previous session whose stream is gone or unresponsive
- **THEN** within one bounded window without a page reload, the composer becomes usable again, either resumed or reset with an explicit error/retry affordance instead of an eternal "thinking" state

#### Scenario: Upstream LLM goes silent mid-stream
- **WHEN** an active stream stops emitting events for longer than the bounded window
- **THEN** the session shows an explicit error state with cancel and retry controls, and the composer unlocks

#### Scenario: User cancels a wedged wait
- **WHEN** the user presses cancel while the bounded wait is still running
- **THEN** the in-flight wait aborts cleanly, the composer unlocks, and exactly one send occurs per user intent (cancel must not duplicate the request; retry re-sends the same message)

### Requirement: All visible UI strings SHALL come from the active locale catalog

Every user-visible literal — labels, filter enumeration values, empty-state titles, label prefixes — SHALL render through the i18n dictionary of the active locale. Raw dictionary keys or hardcoded English SHALL NOT appear on zh-TW pages.

#### Scenario: Documents status filter in zh-TW
- **WHEN** the Documents list renders its status filter values under the zh-TW locale
- **THEN** values display translated labels (e.g., `indexed` → 已索引), never the raw enumeration string

#### Scenario: Terminology alignment empty state
- **WHEN** the Terms page has no data under the zh-TW locale
- **THEN** its empty-state title and body render fully in zh-TW, not the literal `Empty Terminology`

#### Scenario: Sidebar workspace label
- **WHEN** the sidebar renders the workspace entry under the zh-TW locale
- **THEN** any label prefix (e.g., `Workspace:`) is a dictionary string rendered in the active locale

### Requirement: zh-TW copy SHALL follow Taiwan Mandarin conventions

Visible zh-TW copy SHALL NOT carry PRC-regional terminology. At minimum the observed terms are corrected — 運維→營運, 數據→資料, 大盤→總覽 — and a sweep SHALL cover the same family of terms before this change is archived.

#### Scenario: PRC-terminology sweep
- **WHEN** the zh-TW catalogs and rendered pages are scanned for the sampled PRC terms and their obvious family members
- **THEN** no such variants appear in visible UI copy

### Requirement: Same-name documents SHALL be disambiguated before destructive action

When multiple indexed documents share the same file name, the Documents list SHALL visibly distinguish the entries from one another, and destructive actions (delete) SHALL present a confirmation carrying enough identifying information (source/path summary) for the user to select the intended entry with confidence.

#### Scenario: Duplicate names listed
- **WHEN** the Documents list contains documents with identical file names from different sources
- **THEN** each entry is visually distinguishable at list level (not identical rows differing only on hover)

#### Scenario: Delete one of several same-name documents
- **WHEN** the user initiates deletion of a document that has same-name siblings
- **THEN** the confirmation shows identifying information sufficient to tell the target apart from its siblings, and only the chosen entry is deleted

### Requirement: Host filesystem paths SHALL NOT leak in routine lists

Lists and cards SHALL NOT render absolute host filesystem paths (e.g., `/mnt/data/...`). They SHALL render a source summary instead; the full path SHALL only be obtainable through an explicit user action such as expand or details.

#### Scenario: Browsing the documents list
- **WHEN** the user browses indexed documents in the list or its cards
- **THEN** no absolute host path is rendered in the routine view

#### Scenario: Explicit path reveal
- **WHEN** the user explicitly invokes the reveal/details affordance for an entry
- **THEN** the full path becomes available (displayed or copyable) for that entry only

### Requirement: Retrieval preference SHALL have a single stateful source

The retrieval preference (速度/平衡/精準 ↔ Fast/Balanced/Precise) SHALL be stored in exactly one stateful location. The chat-page selector and the Settings-page control SHALL both read and write that single state, and each SHALL visibly reflect changes made at the other without a reload.

#### Scenario: Change from Settings reflects in chat
- **WHEN** the user changes the Query Profile on the Settings page and returns to the chat page
- **THEN** the chat selector shows the same value without a page reload

#### Scenario: Change from chat reflects in Settings
- **WHEN** the user changes the profile from the chat page and opens Settings
- **THEN** the Settings control shows the same value without a page reload

### Requirement: Empty-state copy SHALL match its own panel's data source

Each chart or panel SHALL provide its own empty-state copy describing its specific data source. A generic message about another component's data (e.g., the corpus has no indexed data) SHALL NOT be reused for unrelated panels.

#### Scenario: LLM path distribution with no data
- **WHEN** the activity log's LLM path distribution chart has no records
- **THEN** that chart shows empty-state copy about LLM path data, and corpus-copy appears only where corpus state is actually the subject

### Requirement: Explanatory copy SHALL be single-instance and neutral

Informational explanation blocks SHALL render once per page context (no repeated consecutive duplicates), and product copy SHALL be factual; marketing-tone claims (e.g., 「100% 精準對齊！」) SHALL NOT appear in functional pages.

#### Scenario: Terminology alignment intro
- **WHEN** the Terms page renders its explanatory content
- **THEN** each explanation appears once and no marketing-tone exclamation strings render anywhere on functional pages

### Requirement: Workspace list SHALL bound identifier exposure and interaction safety

The workspace list SHALL hide machine identifiers (UUIDs) and CLI command hints by default; they SHALL appear only via explicit user action. A disabled primary action SHALL retain a minimum label contrast (WCAG AA 4.5:1). A destructive button SHALL keep a spacing buffer from adjacent elements and MUST NOT violate the AGENTS §9 wrapping contract (no clipped labels, no horizontal overflow, no button text wrapping).

#### Scenario: Default list view
- **WHEN** the user opens the workspace list
- **THEN** no UUID or CLI hint text is rendered by default

#### Scenario: Identifier reveal
- **WHEN** the user explicitly invokes the reveal/copy affordance for a workspace
- **THEN** the identifier/CLI hint becomes available for that entry

#### Scenario: Disabled create action and delete placement
- **WHEN** the create action is disabled while a delete button sits next to session rows
- **THEN** the disabled label still meets 4.5:1 contrast, the delete button holds a spacing buffer from adjacent elements, and no wrapping/overflow violation occurs
