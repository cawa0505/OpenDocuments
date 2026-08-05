# OpenSpec Requirement: Workspace Resolution & Isolation

**Spec ID**: `workspace-management`  
**Status**: Approved / Production  
**Priority**: P0  
**Primary Language**: English  

---

## 1. Overview & Core Objective

This specification defines the workspace lifecycle, resolution hierarchy, and data isolation model across OpenDocuments storage and API layers. All document chunks, search queries, and chat threads MUST be strictly scoped to a valid workspace UUID.

---

## 2. System Contracts & Requirements

### 2.1 Default Workspace Guarantee
- Upon server startup, the system MUST inspect the configured `default_workspace` (defined in `~/.config/opendocuments/config.toml`).
- If the default workspace record does not exist in the SQLite `workspaces` table, the system MUST automatically create it.
- The `default_workspace` is protected and MUST NOT be deleted.

### 2.2 Resolution Hierarchy
When resolving a workspace context for CLI commands or REST API requests (`X-Workspace` header), resolution MUST strictly follow this precedence order:
1. **Explicit Flag / Header**: Explicit `--workspace` argument or `X-Workspace` header value (matching UUID or exact workspace name).
2. **Active Workspace**: Persisted `model.active_workspace` setting in `config.toml`.
3. **Default Workspace**: Fallback to `model.default_workspace` setting in `config.toml`.

### 2.3 Foreign Key Integrity
- Empty or missing `X-Workspace` HTTP headers MUST be resolved to a valid workspace UUID before executing database queries to prevent Foreign Key constraint violations (SQLite code 787).

---

## 3. Behavior Specifications

```spec
WHEN the server starts for the first time on a fresh database
THEN the system MUST verify and auto-create the configured default workspace row.

WHEN an API request contains `X-Workspace: homelab`
THEN the system MUST resolve "homelab" against `workspaces` table by name or ID and scope all queries to its corresponding workspace UUID.

WHEN a CLI command executes `opendoc workspace switch <name>`
THEN the system MUST validate `<name>`, update `model.active_workspace` in `config.toml`, and persist the configuration.
```
