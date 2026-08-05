# OpenSpec Requirement: Modular Document Ingestion & Parsers

**Spec ID**: `document-parsers`  
**Status**: Approved / Production  
**Priority**: P1  
**Primary Language**: English  

---

## 1. Overview & Core Objective

This specification defines the sandboxed document parser architecture (`crates/opendoc-parser-*`). OpenDocuments supports native, zero-external-dependency extraction of text and metadata across diverse document formats.

---

## 2. Supported Formats & Parser Crates

| Format / Extension | Parser Crate | Extraction Capabilities |
| :--- | :--- | :--- |
| **PDF** (`.pdf`) | `opendoc-parser-pdf` | Text extraction, page-level chunking |
| **Microsoft Word** (`.docx`) | `opendoc-parser-docx` | Paragraph text, heading structure, table rows |
| **Microsoft Excel** (`.xlsx`) | `opendoc-parser-xlsx` | Sheet names, tabular row/column data |
| **PowerPoint** (`.pptx`) | `opendoc-parser-pptx` | Slide text, slide titles |
| **HTML / Web Pages** (`.html`, `.htm`) | `opendoc-parser-html` | DOM cleaning, main content extraction |
| **Email Messages** (`.eml`, `.msg`) | `opendoc-parser-email` | Subject, sender, body text, attachment lists |
| **Code & Jupyter** (`.rs`, `.py`, `.ipynb`) | `opendoc-parser-code`, `opendoc-parser-jupyter` | Source code lines, cell inputs/outputs |

---

## 3. Ingestion Behavior Contract

```spec
WHEN a document is submitted via `opendoc document index <path>` or WebUI upload
THEN the system MUST route the file to its corresponding `opendoc-parser-*` crate based on MIME type or extension.

WHEN parsing completes
THEN text MUST be split into semantic chunks (`DocumentChunk`), embedded into LanceDB vectors, indexed in SQLite FTS5, and stored with source file metadata.

WHEN an unsupported file type or corrupted file is encountered
THEN the document status MUST be marked as `failed` with an explicit error message in SQLite without crashing the background worker.
```
