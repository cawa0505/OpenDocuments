import type { Context } from 'hono'
import type { AppContext, WorkspaceServices } from '../bootstrap.js'

export function resolveRequestWorkspaceId(
  c: Context,
  ctx: AppContext,
  requestedWorkspaceId?: string
): string {
  const auth = c.get('auth') as { record?: { workspaceId?: string } } | null
  const authWorkspaceId = auth?.record?.workspaceId
  if (authWorkspaceId) return authWorkspaceId

  // --- Dynamic Header & Query Override ---
  const headerWorkspace = c.req.header('x-workspace') || c.req.header('x-workspace-id') || c.req.header('x-workspace-name')
  const queryWorkspace = c.req.query('workspace') || c.req.query('workspaceId')
  const wsParam = requestedWorkspaceId || headerWorkspace || queryWorkspace

  if (wsParam) {
    const requested =
      ctx.workspaceManager.getById(wsParam) ??
      ctx.workspaceManager.getByName(wsParam)
    if (requested) return requested.id

    try {
      const created = ctx.workspaceManager.create(wsParam)
      return created.id
    } catch {}
  }
  // ----------------------------------------

  if (ctx.config.workspace) {
    const configured = ctx.workspaceManager.getByName(ctx.config.workspace)
    if (configured) return configured.id
  }

  return ctx.workspaceManager.ensureDefault().id
}

export function getWorkspaceServices(
  c: Context,
  ctx: AppContext,
  requestedWorkspaceId?: string
): WorkspaceServices {
  return ctx.forWorkspace(resolveRequestWorkspaceId(c, ctx, requestedWorkspaceId))
}
