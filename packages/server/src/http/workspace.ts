import type { Context } from 'hono'
import type { AppContext, WorkspaceServices } from '../bootstrap.js'

// --- Homelab Mesh Host IP Map ---
const MESH_HOST_MAP: Record<string, string> = {
  '192.168.212.200': 'arhat',
  '192.168.77.200': 'arhat',
  '192.168.212.185': 'cybertron',
  '192.168.77.185': 'cybertron',
  '192.168.212.141': 'bumblebee',
}

function getClientIp(c: Context): string | null {
  const forwarded = c.req.header('x-forwarded-for') || c.req.header('x-real-ip')
  if (forwarded) {
    // 處理多重代理的情況，拿第一個 IP
    return forwarded.split(',')[0].trim()
  }
  // Fallback to connection info if available
  const rawReq = c.env?.incoming
  if (rawReq?.socket?.remoteAddress) {
    const ip = rawReq.socket.remoteAddress
    // 移除 IPv6 對映前綴 ::ffff:
    return ip.startsWith('::ffff:') ? ip.substring(7) : ip
  }
  return null
}

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
  let wsParam = requestedWorkspaceId || headerWorkspace || queryWorkspace

  // --- Dynamic Mesh IP-to-Workspace Alignment ---
  if (!wsParam) {
    const clientIp = getClientIp(c)
    if (clientIp && MESH_HOST_MAP[clientIp]) {
      const detectedHost = MESH_HOST_MAP[clientIp]
      // 自動將無 header 請求路由至主機專屬工作空間 (e.g. "arhat" or "cybertron")
      wsParam = detectedHost
    }
  }

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
