# 任務執行層與原生 AI 引擎 — 技術驗證

**狀態**: 參考文件 — 2026-08-09 驗證
[English](../en/task-execution-ai-engines-verification.md) | **繁體中文**
**對應規格**: [`openspec/specs/task-execution-ai-engines/spec.md`](../../../openspec/specs/task-execution-ai-engines/spec.md)

本文件記錄任務執行層與原生 AI 引擎規格所依據的外部技術限制與架構決策，供實作階段（Phase 0 起）直接引用事實與來源，無需重新推導。

---

## 1. 已驗證限制 (T1–T7)

| # | 限制 | 來源 |
|---|------|------|
| T1 | AMD Spur 是 **Slurm 相容的批次排程器**，而非任務佇列框架。元件：控制器 `spurctld`（gRPC :6817）、代理 `spurd`（gRPC :6818）、REST 閘道 `spurrestd`。尚在 1.0 之前（v0.9.0）。任務為 shell 批次作業；無 pull-based worker 佇列、無結構化任務/結果協定。 | github.com/ROCm/spur |
| T2 | ONNX Runtime **無 Vulkan execution provider**（#10603、#21917 仍開啟）。ROCm EP 已在 **ORT ≥ 1.23 移除**；替代的 MIGraphX EP **僅限 Instinct (CDNA)**——消費級 Radeon 不支援。`ort` crate 未提供 ROCm/MIGraphX 預編譯 binary。 | onnxruntime.ai 文件；ONNX Runtime GitHub |
| T3 | Candle (huggingface) 與 mistral.rs **無 AMD GPU backend**——僅 CPU/CUDA/Metal。Rust 原生在 AMD GPU 上以這些 crate 推理並不存在。 | huggingface/candle README |
| T4 | llama.cpp 支援 **Vulkan 與 HIP** backend（Radeon 與 Instinct 皆可）、embedding 模式（`--embedding`）、與 **`/rerank` endpoint**（已合併，#8555 關閉）。bge-m3 embedding 與 bge-reranker 系列 rerank 皆以 GGUF 運行。 | ggml-org/llama.cpp；issue #8555 |
| T5 | fastembed-rs (Anush008) 涵蓋 bge-m3（dense + sparse + ColBERT）與 BGE/Jina reranker，皆為 **ONNX CPU**。預設量化 `BGEM3Q` 模型天生僅限 CPU。 | Anush008/fastembed-rs |
| T6 | bge-m3 dense 向量 = 1024 × f32 ≈ 4 KB 二進位 ≈ 10–12 KB JSON。1 GbE 區網下推理延遲為主，序列化並非瓶頸。v1 採用 JSON-first 線格式可行。 | 計算（向量尺寸）；專案區網假設 |
| T7 | Spur 內部控制面為 gRPC/Protobuf，外部閘道為 JSON REST。Text Embeddings Inference (TEI) 同時以 HTTP JSON 與 gRPC 提供相同 API。設計為可 1:1 對應未來 `.proto` 的 JSON envelope 符合此成熟模式。 | github.com/ROCm/spur；huggingface/text-embeddings-inference |

## 2. 架構決策（與使用者共同決定）

| # | 決策 | 理由 |
|---|------|------|
| D1 | Spur = **批次運算引擎**（worker 生命週期 + 批次 ETL），**非** LAN 任務路由層。 | T1：它是排程器，不是任務佇列。 |
| D2 | **Realtime RAG 永不 per-request 走 `spur run`。** Realtime 使用 Mode 1（常駐 daemon，RPC）或 InProcess。 | 每次查詢冷啟動 `spur run` 不可接受。 |
| D3 | 原生 AI 引擎主力：**llama.cpp**（GGUF，Vulkan/HIP），經 `llama-cpp-rs` / C FFI 整合——embedding、rerank、小型 LM 生成三者皆涵蓋。 | T4：唯一覆蓋三種負載的原生 AMD GPU 路徑。 |
| D4 | 原生 AI 引擎 CPU 備援：**fastembed-rs**（ONNX CPU）——零 C++ FFI，乾淨的單一二進位預設。 | T2/T5：ORT 在 AMD 上本就為 CPU；fastembed 是受維護的 CPU 路徑。 |
| D5 | LAN 任務載荷：**JSON-first 版本化 envelope**（`TaskEnvelope`），protobuf 延後至大量/串流熱路徑出現。 | T6/T7：v1 規模下 JSON 成本可接受；envelope 已設計為 protobuf-mappable。 |
| D6 | 生成保持可插拔：設定 `[ai.models.inference]` 時用 llama.cpp 原生 SLM，否則沿用既有 BYOK OpenAI 相容供應商（opendoc-llm 不變）。 | 與 BYOK 契約向後相容。 |
| D7 | **設定檔維持 `config.toml`**（XDG：`~/.config/opendocuments/config.toml`）；不更名為 `OpenDocuments.toml`。新增 `[ai]`/`[task]` 段落擴充之。 | XDG 合規；向後相容；2026-08-09 使用者決策。 |

## 3. 明確排除的死路

| 路徑 | 排除原因 |
|------|----------|
| ONNX Runtime + Vulkan EP | 不存在（T2）。 |
| ONNX Runtime + ROCm on Radeon | ROCm EP ≥ 1.23 移除；MIGraphX 僅限 Instinct（T2）。 |
| Candle / mistral.rs 於 AMD GPU | 無 AMD backend（T3）。 |
| TEI 作為 in-process 引擎 | Sidecar 程序——違反單一二進位執行期限制（#1445/#1994）。 |
| v1 LAN 載荷即用 Protobuf | 過早；JSON envelope 已足夠，直到大量/串流路徑出現（T6, D5）。 |

## 4. 未決項目（追蹤於規格 §10）

- R1：llama.cpp `/rerank` 輸出品質 vs 既有 ONNX `bge-reranker-base.onnx`——於 Phase 2 gate 基準測試。
- R2：`llama-cpp-rs` C++ 建置成本（cmake 工具鏈、編譯時間）vs sidecar——預設為 feature-gated FFI。
- R3：Spur pre-1.0 HA 語意——SpurDaemonExecutor 必須在 RPC 失敗時回退 InProcess。
- R4：bge-m3 sparse/ColBERT 延後；v1 僅 dense（dim 1024）。
- R5：v1 中 ONNX reranker（fastembed）與 GGUF reranker（llama.cpp）皆由 `AiEngine` 支援。
