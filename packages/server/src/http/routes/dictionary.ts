import { Hono } from 'hono'
import type { AppContext } from '../../bootstrap.js'
import { getWorkspaceServices } from '../workspace.js'
import { requireScope } from '../middleware/auth.js'

export function dictionaryRoutes(ctx: AppContext) {
  const app = new Hono()

  app.get('/api/v1/dictionary', (c) => {
    const { dictionaryManager } = getWorkspaceServices(c, ctx)
    return c.json({ entries: dictionaryManager.list() })
  })

  app.post('/api/v1/dictionary', requireScope('document:write'), async (c) => {
    const { dictionaryManager } = getWorkspaceServices(c, ctx)
    const body = await c.req.json<{ key: string; value: string }>()
    const key = body.key?.trim()
    const value = body.value?.trim()

    if (!key || !value) {
      return c.json({ error: 'Both key and value are required' }, 400)
    }

    try {
      const entry = dictionaryManager.upsert(key, value)
      return c.json(entry, 201)
    } catch (err: any) {
      return c.json({ error: err.message || 'Failed to save dictionary entry' }, 500)
    }
  })

  app.post('/api/v1/dictionary/import-seed', requireScope('document:write'), async (c) => {
    const { dictionaryManager } = getWorkspaceServices(c, ctx)
    const body = await c.req.json<{ language: 'zh-TW' | 'ko-KR' }>()
    const language = body.language
    
    if (language !== 'zh-TW' && language !== 'ko-KR') {
      return c.json({ error: 'Language must be either "zh-TW" or "ko-KR"' }, 400)
    }

    try {
      dictionaryManager.importSeed(language)
      return c.json({ imported: true })
    } catch (err: any) {
      return c.json({ error: err.message || 'Failed to import dictionary seed' }, 500)
    }
  })

  app.delete('/api/v1/dictionary/:id', requireScope('document:write'), (c) => {
    const { dictionaryManager } = getWorkspaceServices(c, ctx)
    const id = c.req.param('id')
    if (!id) return c.json({ error: 'Dictionary entry ID required' }, 400)
    
    dictionaryManager.delete(id)
    return c.json({ deleted: true })
  })

  return app
}
