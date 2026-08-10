# OpenSpec Requirement: Binary Size Architecture

**Spec ID**: `binary-size-architecture`
**Status**: Draft — Pending Approval
**Priority**: P0
**Primary Language**: English
**Last Updated**: 2026-08-10
**Source**: Oracle independent architecture review (§701)

---

## 1. Overview & Core Objective

Define enforceable binary size boundaries for the unified native OpenDocuments binary (Axum + LanceDB + SQLite + parsers + BYOK LLM + CLI). The goal is to prevent unbounded growth and make size regression visible in CI. No new features, no AWS/plugin sidecar design — only measurable limits, feature boundaries, and gate rules.

**Current measured baseline (lancedb 0.23.1, local-only, release build):**

| Metric | Value |
|---|---:|
| Release unstripped | 413,321,376 bytes (394.2 MiB) |
| `strip --strip-all` | 210,007,192 bytes (200.3 MiB) |
| RSS at startup (model cache excluded) | ~180–220 MiB |

**Prior state (lancedb 0.10 with forced DynamoDB/AWS):** 295,076,376 bytes unstripped.  
**Worst case (lancedb 0.33 local-only):** 499,739,360 bytes unstripped.

---

## 2. Binary Budgets

| Artifact | Hard Budget | Rationale |
|---|---:|---|
| **Core binary (stripped, release)** | ≤ 150 MiB | Target leaves headroom for future optional features; aligned with home-lab single-binary distribution and CI gate at 160 MiB. |
| **RSS at idle (no model cache)** | ≤ 220 MiB | Observed range; prevents silent memory bloat from static buffers. |
| **Model cache (`<db_dir>/models`)** | ≤ 500 MiB per model | Outside binary; documented and user-configurable. |
| **Optional feature (TUI / WebUI / fastembed)** | ≤ 30 MiB each | Must be feature-gated; enables `cargo build --release --features <X>` size verification. |

---

## 3. Feature Boundaries

| Category | In Core | Optional (feature-gated) | Future Plugin (separate crate/process) |
|---|---|---|---|
| **Parsers** | PDF, DOCX, XLSX, PPTX, HTML, Email, Text | — | — |
| **Vector store** | LanceDB (local, file-manifest) | — | DynamoDB/S3 external manifest |
| **Embedding** | BYOK (OpenAI-compatible `/v1/embeddings`) | FastEmbed CPU (`embedding-fastembed` feature) | GPU (llama.cpp, ONNX Runtime CUDA) |
| **Interfaces** | CLI, REST API, MCP server | TUI (`tui` feature), WebUI (`webui` feature) | — |
| **Auth/License** | Local JWT, workspace isolation | — | Hardware fingerprint, clock-skew (loom-security) |
| **Observability** | Structured logs | Metrics endpoint | Distributed tracing exporters |

**Rule:** New code that would cross a boundary MUST first be added as an optional feature or plugin proposal with size impact documented.

---

## 4. CI Size Regression Gates

### 4.1 Measurement Procedure (deterministic, scriptable)

```bash
# scripts/measure-size.sh
set -euo pipefail
cargo build --release --locked
STRIPPED=$(mktemp)
cp target/release/opendoc "$STRIPPED"
strip --strip-all "$STRIPPED"
SIZE=$(stat -c%s "$STRIPPED")
echo "stripped_bytes=$SIZE"
echo "stripped_mib=$((SIZE / 1024 / 1024))"
# Fail gate
if [ $SIZE -gt 167772160 ]; then  # 160 MiB
  echo "ERROR: stripped binary $SIZE bytes exceeds 160 MiB gate"
  exit 1
fi
```

### 4.2 CI Integration

- Add as a **required** job in release workflow (runs on every PR + push to main).
- Artifact the stripped binary for audit trail.
- Record stripped size + RSS sample in job summary (Markdown table).

### 4.3 Gate Thresholds

| Check | Threshold | Action |
|---|---|---|
| Stripped binary | > 160 MiB | **Fail build** (hard gate; 150 MiB target + 10 MiB headroom) |
| Unstripped binary | > 450 MiB | Warn only (tracking) |
| RSS idle sample | > 220 MiB | Warn only |

---

## 5. Dependency Upgrade Admission Rules

| Change Type | Required Evidence |
|---|---|
| **Minor/Patch (lancedb, arrow, datafusion, tokio, etc.)** | CI gate passes; no new transitive `aws-*`, `google-cloud-*`, `azure-*` crates added. |
| **Major version (lance 1.x → 2.x, datafusion 50 → 60, etc.)** | 1. Local stripped build ≤ 150 MiB. 2. All existing tests pass. 3. No new non-Rust build deps (protoc already allowed). 4. User approval recorded in PR. |
| **New optional feature (e.g. `embedding-fastembed`, `tui`, `webui`)** | Size delta measured and documented in PR; default feature set MUST NOT enable it. |
| **Any dependency pulling `cc`, `pkg-config`, or system libs** | Must be optional feature; justification in PR. |

---

## 6. Phased Execution (Highest Leverage, Lowest Risk First)

| Phase | Scope | Validation |
|---|---|---|
| **0 — Immediate (this PR)** | Add `scripts/measure-size.sh` + CI gate at 160 MiB; document current 200.3 MiB baseline as accepted tech debt with target milestone. | Gate runs, records size; build passes (gate is 160 MiB, current 200 MiB → build will fail; must raise gate to 210 MiB for now with milestone comment). |
| **1 — Profile/Strip Optimization (next 2 weeks)** | Enable `strip = "symbols"` (or `--strip-all` post-build), `lto = "thin"`, `codegen-units = 1` in `[profile.release]`; A/B measure size + build time. | If ≥ 15 MiB saved with ≤ 2× build time → keep; else revert. |
| **2 — Dependency Attribution & Pruning** | Run `cargo bloat --release` (or equivalent) to get per-crate top-5 contributors; drop unused features (`default-features=false` where safe). | Each prune must pass CI gate + full test suite. |
| **3 — Feature Gating** | Move WebUI/TUI to optional features (already partial); verify core-only build ≤ 150 MiB. | Core build passes 160 MiB gate. |
| **4 — Future Plugin Boundary (deferred)** | Only when concrete remote/storage need exists; define plugin protocol; keep core ≤ 150 MiB. | No action until approved need. |

---

## 7. Risks & Decisions Requiring Approval

| Risk | Mitigation | User Approval Needed |
|---|---|---|
| **lancedb 0.24+ may increase size despite optimizations** | CI gate blocks; test locally before upgrade PR. | Yes — major version upgrade requires explicit go-ahead. |
| **FastEmbed default-on would bloat core** | Keep `embedding-fastembed` feature OFF by default; document. | Confirm default-off. |
| **Single-binary constraint limits parallel/horizontal scaling** | Accepted for home-lab; plugin architecture for future scale. | Acknowledged; no change. |
| **Protoc availability in CI** | Already in release matrix (`arduino/setup-protoc@v3`). | None (existing). |
| **RSS growth from static Arrow/DataFusion buffers** | Monitor via CI RSS sample; if sustained > 220 MiB, investigate. | None (monitoring). |

---

## 8. Inspected Paths & References

- `Cargo.toml` (root), `crates/*/Cargo.toml`
- `openspec/specs/single-binary-architecture/spec.md` (existing)
- `docs/en/structure.md` (architecture diagram)
- Oracle review session: `ses_0156313acffeuIpXVB0rJBAg3H`

---

## 9. Definition of Done (for this spec)

1. Spec approved (status → Approved).
2. `scripts/measure-size.sh` added and CI gate integrated at documented threshold.
3. Phase 0 baseline accepted with milestone comment (gate = 210 MiB until Phase 1 lands).
4. All subsequent PRs must pass CI size gate; optional feature size deltas documented.

---

**End of Draft**