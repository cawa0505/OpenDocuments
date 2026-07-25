import { Hono } from 'hono'
import { SSEServerTransport } from '@modelcontextprotocol/sdk/server/sse.js'
import { RESPONSE_ALREADY_SENT } from '@hono/node-server/utils/response'
import type { HttpBindings } from '@hono/node-server'
import type { AppContext } from '../../bootstrap.js'
import { createMCPServer } from '../../mcp/server.js'

export function mcpRoutes(ctx: AppContext) {
  const app = new Hono()
  const mcpServer = createMCPServer(ctx, 'read')
  const activeTransports = new Map<string, SSEServerTransport>()

  app.get('/mcp/sse', async (c) => {
    const { outgoing } = c.env as HttpBindings
    
    // SSEServerTransport takes message endpoint and raw node http response
    const transport = new SSEServerTransport('/mcp/message', outgoing)
    activeTransports.set(transport.sessionId, transport)

    outgoing.on('close', () => {
      activeTransports.delete(transport.sessionId)
    })

    await mcpServer.connect(transport)
    return RESPONSE_ALREADY_SENT
  })

  app.post('/mcp/message', async (c) => {
    const sessionId = c.req.query('sessionId')
    const transport = activeTransports.get(sessionId || '')
    if (!transport) {
      return c.text('Session not found', 400)
    }

    const { incoming, outgoing } = c.env as HttpBindings
    await transport.handlePostMessage(incoming, outgoing)
    return RESPONSE_ALREADY_SENT
  })

  return app
}
