# OpenSpec Requirement: BYOK LLM Layer & Secrets Isolation

**Spec ID**: `byok-llm-providers`  
**Status**: Approved / Production  
**Priority**: P0  
**Primary Language**: English  

---

## 1. Overview & Core Objective

This specification defines the Bring-Your-Own-Key (BYOK) LLM provider module (`opendoc-llm`). OpenDocuments supports OpenAI-compatible API providers (OpenAI, Claude, Ollama, LiteLLM, vLLM) with SSE progressive streaming while enforcing strict zero-trust secrets isolation.

---

## 2. System Contracts & Requirements

### 2.1 Encryption & Secrets Storage
- API keys MUST NOT be persisted in system environment variables or plain text config files.
- Provider credentials (name, provider_type, base_url, model, encrypted_api_key) MUST be stored in the SQLite `llm_providers` table.
- Plaintext API keys MUST NEVER be echoed or exposed back to the WebUI frontend in REST responses.

### 2.2 SSE Progressive Streaming (`StreamEvent`)
- Chat completions MUST stream progressive Server-Sent Events (SSE) structured as `StreamEvent` JSON objects:
  - `Thought`: Intermediate LLM thinking/reasoning output (collapsible in WebUI).
  - `Text`: Generated answer tokens for typewriter streaming rendering.
  - `Status`: Execution milestones, citation sources, or error messages.

### 2.3 Provider Diagnostics
- The API MUST provide a connection health check endpoint (`POST /api/v1/admin/llm/providers/test-connection`) to validate provider endpoints and keys before enabling them.

---

## 3. Behavior Specifications

```spec
WHEN an LLM provider is registered via `POST /api/v1/admin/llm/providers`
THEN the system MUST store credentials in SQLite `llm_providers` and return the record with the API key masked (e.g. `sk-***`).

WHEN a user initiates a chat session
THEN `opendoc-llm` MUST issue an HTTP POST request to the provider's `chat/completions` endpoint and stream `StreamEvent` tokens via SSE.

WHEN an invalid API key or unreachable Base URL is configured
THEN `test-connection` MUST return HTTP 400 Bad Request with a clear diagnostic error message.
```
