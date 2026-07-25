# OpenDocuments Rust Refactor Specification — Phase 1 & 2

## ADDED Requirements

### Requirement: Strong-typed Chunking and Processing Contract
The system SHALL define clear, serializable types for all document ingestion chunks, restricting input mutations in downstream parsers, search logic, and TUI representations.

#### Scenario: Instantiating a semantic chunk
- **WHEN** a parser processes an document chunk
- **THEN** it SHALL output a strict `DocumentChunk` struct mapping:
  - `chunk_type` as either `Semantic`, `CodeAst`, or `Table`
  - `content` as non-empty String
  - `workspace_id` and `collection_id` as String tags
  - `relevance_score` optionally evaluated during post-reranking
  - `metadata` containing JSON-structured document context

---\n\n### Requirement: Standard Plugin API Alignment & Trait Contract\nThe Rust implementation SHALL strictly match the architectural intent of the original TypeScript `ParserPlugin` interface. It must define a shared `DocumentParser` trait contract, allowing independent feature-isolated parser crates to be registered and queried dynamically.\n\n- Each parser crate (e.g., `opendoc-parser-xlsx`, `opendoc-parser-html`) MUST implement `DocumentParser`.\n- The total parser router (`opendoc-parser`) MUST act as the central registry, resolving extensions to their corresponding trait instances.\n\n---\n\n### Requirement: Event-Loop-Friendly Binary File Parsers (xlsx, docx, pdf)
The system SHALL implement Pure-Rust document parsing engines that run without blocking the core runtime event-loop.

#### Scenario: Ingesting an Excel sheet
- **WHEN** calamine parsing is invoked on a large `.xlsx`
- **THEN** it SHALL extract headers, estimate tokens using character weight heuristics, and output partitioned `ChunkType::Table` blocks.

#### Scenario: Ingesting a Word document
- **WHEN** docx-rs parsing runs on a `.docx`
- **THEN** it SHALL build a parent header stack (H1-H6) and prepend hierarchical header chains to each child paragraph chunk's metadata.

---

### Requirement: Resilience-Engineered File Ingestion & Parser Routing
The system SHALL guarantee file ingestion robustness, handling randomized temporary filenames (without extensions), uppercase/stray file extensions, and file permission boundaries seamlessly.

#### Scenario: Ingesting a PDF with uppercase extension and missing system association
- **WHEN** the system is requested to parse a file named `/tmp/UPLOADS_GUIDE.PDF`
- **THEN** it SHALL normalize the file extension to lowercase and correctly route the file to `PdfParser`.

#### Scenario: Ingesting a temporary file with randomized hash and no suffix
- **WHEN** the system receives an upload written to `/tmp/83af128bcde` with `original_name = Some("financial_report.xlsx")`
- **THEN** it SHALL extract the file extension from the original name fallback, map it to `ExcelParser`, and successfully complete chunking.

---

### Requirement: Workspace-Isolated LanceDB Vector Search
The system SHALL interact with LanceDB using native Rust bindings, ensuring dynamic table isolation per workspace.

#### Scenario: Storing vectors for a workspace
- **WHEN** vectors are persisted to storage for `workspace_id = "proj_a"`
- **THEN** the system SHALL create or connect to a separate isolated LanceDB table named `opendocuments_workspace_proj_a`.

---

### Requirement: Double-Stage Reranker & Score Threshold Filter Fuse
The system SHALL evaluate a multi-tier query pipelines containing:
1. Fast heuristic keyword/path-based weight filtering.
2. Pairwise LLM/Cross-Encoder semantic scoring.
3. Score Threshold Filter "Fuse" pruning out any candidates under the specified score.

#### Scenario: Filtering results with dynamic threshold
- **WHEN** query results are returned from LanceDB vector index
- **THEN** the Score Filter SHALL discard results below `SearchQueryParams.score_threshold` (e.g., 0.70), but SHALL gracefully fall back to preserving the top-1 result if all items would otherwise be filtered out.

---

### Requirement: Low-Latency Ratatui TUI Debug & Search Terminal
The system SHALL expose an interactive, terminal-native user interface rendered using Ratatui + Crossterm, allowing rapid search querying and ingestion progress monitoring.

#### Scenario: Querying from the terminal
- **WHEN** you input a query into the TUI search box
- **THEN** it SHALL fetch top-5 results within <10ms and render them in a clean visual table showing Score, Workspace, and Content snippets.
