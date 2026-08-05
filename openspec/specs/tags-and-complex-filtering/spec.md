# OpenSpec Requirement: Document Tags & Complex Filtering

**Spec ID**: `tags-and-complex-filtering`  
**Status**: Approved / Production  
**Priority**: P1  
**Primary Language**: English  

---

## 1. Overview & Core Objective

This specification defines document tagging CRUD operations, multi-attribute filtering, and dynamic multi-column sorting across document collections.

---

## 2. API Endpoints & Contracts

### 2.1 Tag Management Endpoints
- `GET /api/v1/tags`: List all tags in active workspace.
- `POST /api/v1/tags`: Create a new tag (name, color).
- `DELETE /api/v1/tags/:id`: Delete a tag and remove document associations.
- `POST /api/v1/documents/:docId/tags/:tagId`: Attach tag to document.
- `DELETE /api/v1/documents/:docId/tags/:tagId`: Detach tag from document.

### 2.2 Complex Query & Sorting Requirements
- Document list endpoints MUST support multi-attribute filtering by `status` (`indexed`, `pending`, `failed`), `sourceType`, and `tagId`.
- Query parameters MUST support dynamic multi-column sorting (`sort_by=title|updated_at|created_at`, `order=asc|desc`).

---

## 3. Behavior Specifications

```spec
WHEN a tag is attached to a document via `POST /api/v1/documents/:docId/tags/:tagId`
THEN the relation MUST be recorded in SQLite `document_tags` table with Foreign Key validation.

WHEN a request fetches `GET /api/v1/documents?status=indexed&sort_by=updated_at&order=desc`
THEN the system MUST construct dynamic SQL queries and return matching records ordered by `updated_at` descending.
```
