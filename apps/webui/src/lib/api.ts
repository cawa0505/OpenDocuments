import type { QueryResult, Document, StatsResponse, AdminStatsResponse, SearchQualityResponse, QueryLogsResponse, PluginHealthResponse, ConnectorStatusResponse, Conversation, ConversationMessage, WorkbenchResponse, Collection, Workspace, LlmProvider, LlmTestResponse } from './types'
import { withStoredApiKey } from './auth'

const BASE = '/api/v1'

async function request<T>(path: string, options?: RequestInit): Promise<T> {
  const { headers, ...rest } = options || {}
    const activeWorkspace = localStorage.getItem('active-workspace') || ''
    const activeLocale = localStorage.getItem('opendocuments-locale') || 'zh-TW'
    const res = await fetch(`${BASE}${path}`, {
      ...rest,
      credentials: 'same-origin',
      headers: withStoredApiKey({ 
        'Content-Type': 'application/json', 
        'X-Workspace': activeWorkspace,
        'X-Locale': activeLocale,
        'Accept-Language': activeLocale,
        ...headers 
      }),
    })
  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: 'Request failed' }))
    throw new Error(body.error || `HTTP ${res.status}`)
  }
  return res.json()
}

// Chat
export async function chat(query: string, profile?: string): Promise<QueryResult> {
  return request('/chat', {
    method: 'POST',
    body: JSON.stringify({ query, profile }),
  })
}

// Documents
export async function listDocuments(): Promise<{ documents: Document[] }> {
  return request('/documents')
}

export async function getDocument(id: string): Promise<Document> {
  return request(`/documents/${id}`)
}

export async function deleteDocument(id: string): Promise<void> {
  await request(`/documents/${id}`, { method: 'DELETE' })
}

export async function uploadDocument(file: File): Promise<{ documentId: string; chunks: number; status: string }> {
  const formData = new FormData()
  formData.append('file', file)
  const activeWorkspace = localStorage.getItem('active-workspace') || ''
  const res = await fetch(`${BASE}/documents/upload`, {
    credentials: 'same-origin',
    headers: withStoredApiKey({
      'X-Workspace': activeWorkspace
    }),
    method: 'POST',
    body: formData,
  })
  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: 'Upload failed' }))
    throw new Error(body.error || `Upload failed with HTTP ${res.status}`)
  }
  return res.json()
}

// Health
export async function getHealth(): Promise<{ status: string; version: string }> {
  return request('/health')
}

export async function getStats(): Promise<StatsResponse> {
  return request('/stats')
}

export async function getWorkbench(): Promise<WorkbenchResponse> {
  return request('/workbench')
}

export async function listWorkspaces(): Promise<{ workspaces: Workspace[] }> {
  return request('/workspaces')
}

export async function deleteWorkspace(id: string): Promise<{ deleted: boolean }> {
  return request(`/workspaces/${encodeURIComponent(id)}`, { method: 'DELETE' })
}

// Conversations
export async function listConversations(opts?: { limit?: number; offset?: number }): Promise<{ conversations: Conversation[]; limit: number; offset: number }> {
  const params = new URLSearchParams()
  if (opts?.limit) params.set('limit', String(opts.limit))
  if (opts?.offset) params.set('offset', String(opts.offset))
  const query = params.toString()
  return request(`/conversations${query ? `?${query}` : ''}`)
}

export async function createConversation(title?: string): Promise<Conversation> {
  return request('/conversations', {
    method: 'POST',
    body: JSON.stringify(title ? { title } : {}),
  })
}

export async function getConversationMessages(id: string): Promise<{ messages: ConversationMessage[] }> {
  return request(`/conversations/${encodeURIComponent(id)}/messages`)
}

export async function updateConversation(id: string, input: { title?: string }): Promise<{ updated: true }> {
  return request(`/conversations/${encodeURIComponent(id)}`, {
    method: 'PATCH',
    body: JSON.stringify(input),
  })
}

export async function deleteConversation(id: string): Promise<{ deleted: true }> {
  return request(`/conversations/${encodeURIComponent(id)}`, { method: 'DELETE' })
}

export async function shareConversation(id: string): Promise<{ shareUrl: string }> {
  return request(`/conversations/${encodeURIComponent(id)}/share`, { method: 'POST' })
}

// Admin
export async function getAdminStats(): Promise<AdminStatsResponse> {
  return request('/admin/stats')
}

export interface VersionCheckResponse {
  current_version: string
  latest_version: string
  has_update: boolean
  update_command: string
}

export async function checkVersion(): Promise<VersionCheckResponse> {
  return request('/admin/version-check')
}

export async function getSearchQuality(): Promise<SearchQualityResponse> {
  return request('/admin/search-quality')
}

export async function getQueryLogs(opts?: { limit?: number; offset?: number }): Promise<QueryLogsResponse> {
  const params = new URLSearchParams()
  if (opts?.limit) params.set('limit', String(opts.limit))
  if (opts?.offset) params.set('offset', String(opts.offset))
  return request(`/admin/query-logs?${params}`)
}

export async function deleteQueryLog(id: string): Promise<{ success: boolean }> {
  return request(`/admin/query-logs/${id}`, { method: 'DELETE' })
}

// Collections
export async function listCollections(): Promise<{ collections: Collection[] }> {
  return request('/collections')
}

export async function createCollection(input: { name: string; description?: string }): Promise<Collection> {
  return request('/collections', {
    method: 'POST',
    body: JSON.stringify(input),
  })
}

export async function deleteCollection(id: string): Promise<{ deleted: true }> {
  return request(`/collections/${encodeURIComponent(id)}`, { method: 'DELETE' })
}

export async function getCollectionDocuments(id: string): Promise<{ collection: Collection; documents: Document[] }> {
  return request(`/collections/${encodeURIComponent(id)}/documents`)
}

export async function addDocumentToCollection(collectionId: string, documentId: string): Promise<{ added: true }> {
  return request(`/collections/${encodeURIComponent(collectionId)}/documents/${encodeURIComponent(documentId)}`, { method: 'POST' })
}

export async function removeDocumentFromCollection(collectionId: string, documentId: string): Promise<{ removed: true }> {
  return request(`/collections/${encodeURIComponent(collectionId)}/documents/${encodeURIComponent(documentId)}`, { method: 'DELETE' })
}

export async function getPluginHealth(): Promise<PluginHealthResponse> {
  return request('/admin/plugins')
}

export async function getConnectorStatus(): Promise<ConnectorStatusResponse> {
  return request('/admin/connectors')
}

export async function connectGitHubConnector(input: {
  repo: string
  token?: string
  branch?: string
  paths?: string[]
  syncInterval?: number
}): Promise<{ connector: ConnectorStatusResponse['connectors'][number]; health: { healthy: boolean; message?: string } }> {
  return request('/admin/connectors/github', {
    method: 'POST',
    body: JSON.stringify(input),
  })
}

export async function syncGitHubConnector(): Promise<{ result: {
  connectorName: string
  documentsDiscovered: number
  documentsIndexed: number
  documentsSkipped: number
  errors: string[]
} }> {
  return request('/admin/connectors/github/sync', { method: 'POST' })
}

interface BenchmarkRun {
  model: string
  metricName: string
  metricValue: number
  createdAt: string
}

export async function getModelBenchmarks(): Promise<{ benchmarks: Array<{
  name: string
  version: string
  capabilities: Record<string, boolean | undefined>
  health: { healthy: boolean; message?: string } | null
  generation: { latencyMs: number; tokensPerSec: number } | { error: string } | null
  embedding: { latencyMs: number; textsPerSec: number } | { error: string } | null
}> }> {
  const data = await request<{ runs: BenchmarkRun[] }>('/admin/benchmark')

  // Group flat runs by model into structured benchmark entries
  const byModel = new Map<string, BenchmarkRun[]>()
  for (const run of data.runs) {
    const existing = byModel.get(run.model) ?? []
    existing.push(run)
    byModel.set(run.model, existing)
  }

  const benchmarks = Array.from(byModel.entries()).map(([name, runs]) => {
    const metrics = new Map(runs.map(r => [r.metricName, r.metricValue]))
    return {
      name,
      version: '1.0',
      capabilities: {} as Record<string, boolean | undefined>,
      health: { healthy: true },
      generation: metrics.has('latencyMs') || metrics.has('tokensPerSec')
        ? { latencyMs: metrics.get('latencyMs') ?? 0, tokensPerSec: metrics.get('tokensPerSec') ?? 0 }
        : null,
      embedding: metrics.has('textsPerSec')
        ? { latencyMs: 0, textsPerSec: metrics.get('textsPerSec')! }
        : null,
    }
  })

  return { benchmarks }
}

// Plugins
export async function searchPlugins(query: string): Promise<{ packages: Array<{ name: string; description: string; version: string; [key: string]: unknown }> }> {
  return request(`/plugins/search?q=${encodeURIComponent(query)}`)
}

export async function getPlugins(): Promise<{ plugins: Array<{ name: string; type: string; version: string; health: { healthy: boolean; message?: string } }> }> {
  return request('/plugins')
}

export async function installPlugin(name: string): Promise<{ status: string; message: string }> {
  return request('/plugins/install', {
    method: 'POST',
    body: JSON.stringify({ name }),
  })
}

export async function removePlugin(name: string): Promise<{ status: string }> {
  return request(`/plugins/${encodeURIComponent(name)}`, { method: 'DELETE' })
}

// Feedback
export async function submitFeedback(queryId: string, feedback: 'positive' | 'negative'): Promise<void> {
  await request('/chat/feedback', { method: 'POST', body: JSON.stringify({ queryId, feedback }) })
}

// Dashboard
export async function getDashboardData() {
  const [stats, adminStats, connectorStatus, pluginHealth] = await Promise.all([
    getStats(), getAdminStats(), getConnectorStatus(), getPluginHealth(),
  ])
  return { stats, adminStats, connectorStatus, pluginHealth }
}

// Dictionary
export interface DictionaryEntry {
  id: string
  workspaceId: string
  key: string
  value: string
  createdAt: string
}

export async function getDictionary(): Promise<{ entries: DictionaryEntry[] }> {
  return request('/dictionary')
}

export async function addDictionaryEntry(key: string, value: string): Promise<DictionaryEntry> {
  return request('/dictionary', {
    method: 'POST',
    body: JSON.stringify({ key, value })
  })
}

export async function deleteDictionaryEntry(id: string): Promise<{ deleted: boolean }> {
  return request(`/dictionary/${encodeURIComponent(id)}`, {
    method: 'DELETE'
  })
}

export async function importDictionarySeed(language: 'zh-TW' | 'ko-KR'): Promise<{ imported: boolean }> {
  return request('/dictionary/import-seed', {
    method: 'POST',
    body: JSON.stringify({ language })
  })
}

// LLM Providers
export async function listLlmProviders(): Promise<{ providers: LlmProvider[] }> {
  return request('/admin/llm/providers')
}

export async function upsertLlmProvider(input: LlmProvider): Promise<LlmProvider> {
  return request('/admin/llm/providers', {
    method: 'POST',
    body: JSON.stringify(input)
  })
}

export async function deleteLlmProvider(id: string): Promise<{ deleted: boolean }> {
  return request(`/admin/llm/providers/${encodeURIComponent(id)}`, {
    method: 'DELETE'
  })
}

export async function testLlmProvider(input: {
  providerId?: string
  baseUrl?: string
  model?: string
  apiKey?: string
}): Promise<LlmTestResponse> {
  return request('/admin/llm/providers/test', {
    method: 'POST',
    body: JSON.stringify(input)
  })
}
