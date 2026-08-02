---
title: Self-Hosted RAG with Ollama
description: Run OpenDocuments as a local-first RAG platform with Ollama via the OpenAI-compatible BYOK settings.
head:
  - - meta
    - name: keywords
      content: self-hosted rag with ollama, local rag, private document qa, ollama document search, local ai knowledge base, open source rag ollama, byok ollama
---

# Self-Hosted RAG with Ollama (BYOK)

OpenDocuments supports running with **Ollama** completely offline and locally. Because our backend architecture leverages a unified **OpenAI-compatible BYOK (Bring Your Own Key)** client, you do not need dedicated Ollama integration code. You can connect to Ollama directly by pointing OpenDocuments to Ollama's OpenAI-compatible API endpoint.

---

## 1. Start Ollama Locally

Ensure Ollama is installed on your system. If not, download it from [ollama.com](https://ollama.com).

Once installed, start the Ollama service and download your preferred model. For example, to run Llama 3:

```bash
# Pull and run your target model
ollama run llama3
```

By default, Ollama serves its API on `http://localhost:11434`. 
Ollama provides an OpenAI-compatible API layer at the route `/v1`. Therefore, your local API endpoint will be:
```plaintext
http://localhost:11434/v1
```

---

## 2. Configure OpenDocuments to use Ollama

Since OpenDocuments stores all LLM provider settings securely in its SQLite backend via BYOK, you can configure it via the CLI or the WebUI settings.

### Option A: Using WebUI Settings
1. Open the OpenDocuments WebUI.
2. Navigate to **Settings** -> **LLM Provider**.
3. Set **API Base URL** (Endpoint) to: `http://localhost:11434/v1`
4. Set **API Key** to: `ollama` (or any non-empty placeholder string, as Ollama doesn't require keys but our validation checks for presence).
5. Set **Model ID** to the exact model name you pulled (e.g., `llama3` or `mistral`).
6. Click **Save & Test Connection**.

### Option B: Using CLI Configuration
You can switch or set your default provider parameters by writing to the `llm_providers` table or through the CLI configure workspace interface.

---

## Recommended Local Models

| System Hardware | Recommended LLM | Recommended Embedding |
|-----------------|-----------------|-----------------------|
| **32GB+ RAM, GPU** | `llama3:8b` or `mistral` | Local ONNX (Built-in) |
| **16GB RAM** | `gemma2:9b` or `llama3:8b` | Local ONNX (Built-in) |
| **8GB RAM** | `qwen2:1.5b` or `phi3` | Local ONNX (Built-in) |

> **Note**: For text embeddings, OpenDocuments leverages its built-in fast ONNX runtime directly inside the single binary process, which processes documents offline with zero external network overhead. Ollama is only used to generate the final chat responses.

---

## Troubleshooting

### Connection Refused (CORS / Host binding)
If your OpenDocuments server is running in a container, a separate virtual machine, or another host, Ollama by default binds to `127.0.0.1` and will reject external connections. 

To allow external connections to Ollama:
* **macOS**:
  ```bash
  launchctl setenv OLLAMA_HOST "0.0.0.0"
  # Restart Ollama application
  ```
* **Linux (systemd)**:
  1. Edit systemd service: `systemctl edit ollama.service`
  2. Add under `[Service]`:
     ```ini
     Environment="OLLAMA_HOST=0.0.0.0"
     ```
  3. Reload and restart:
     ```bash
     sudo systemctl daemon-reload
     sudo systemctl restart ollama
     ```
