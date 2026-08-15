import { useRef, useEffect, useState } from 'react'
import { LoaderCircle } from 'lucide-react'
import { useChatStore } from '../../stores/chatStore'
import { useAppStore } from '../../stores/appStore'
import { ChatInput } from './ChatInput'
import { ChatMessage } from './ChatMessage'
import { streamChat } from '../../lib/sse'
import { getWorkbench, listConversations, submitFeedback, updateConversation, uploadDocument } from '../../lib/api'
import type { WorkbenchResponse } from '../../lib/types'
import { translate as tr } from '../../lib/i18n'

export function ChatPage() {
  const {
    messages,
    isStreaming,
    currentStreamText,
    currentSources,
    currentConfidence,
    conversationId,
    conversations,
    activeError,
  } = useChatStore()
  const { profile, locale } = useAppStore()
  const t = (key: string, values?: Record<string, string | number>) => tr(locale, key, values)
  const bottomRef = useRef<HTMLDivElement>(null)
  const abortRef = useRef<AbortController | null>(null)
  const [workbench, setWorkbench] = useState<WorkbenchResponse | null>(null)
  const [workbenchError, setWorkbenchError] = useState<string | null>(null)
  const [uploading, setUploading] = useState(false)
  const [editingTitle, setEditingTitle] = useState(false)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [editTitle, setEditTitle] = useState('')

  const healthStatus = workbenchError ? 'offline' : 'ready'
  const showPreview = messages.length === 0 && !isStreaming
  const suggestedQuestions = workbench?.suggestedQuestions ?? []
  const activeConversation = conversations.find((conversation) => conversation.id === conversationId)
  const activeConversationTitle = activeConversation?.title || (conversationId ? t('chat.untitled') : t('chat.newChat'))

  useEffect(() => {
    if (bottomRef.current && (!showPreview || messages.length > 0)) {
      bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
  }, [messages, currentStreamText, showPreview, bottomRef]);

  const refreshWorkbench = async () => {
    try {
      const result = await getWorkbench()
      setWorkbench(result)
      setWorkbenchError(null)
    } catch (error) {
      setWorkbenchError(error instanceof Error ? error.message : t('chat.errorWorkbench'))
    }
  }

  const refreshConversations = async () => {
    useChatStore.getState().setConversationsLoading(true)
    try {
      const result = await listConversations({ limit: 80 })
      useChatStore.getState().setConversations(result.conversations)
    } catch (error) {
      useChatStore.getState().setActiveError(error instanceof Error ? error.message : t('chat.errorSessions'))
    } finally {
      useChatStore.getState().setConversationsLoading(false)
    }
  }

  useEffect(() => {
    useChatStore.getState().setActiveError(null)
    void refreshConversations()
    void refreshWorkbench()
  }, [])

  const handleSend = async (query: string) => {
    const store = useChatStore.getState()
    const startingConversationId = store.conversationId
    store.addUserMessage(query)
    store.startStreaming()

    abortRef.current = new AbortController()

    try {
      await streamChat(query, profile, conversationId, {
        onChunk: (text) => useChatStore.getState().appendStreamChunk(text),
        onSources: (sources) => useChatStore.getState().setSources(sources),
        onConfidence: (confidence) => useChatStore.getState().setConfidence(confidence),
        onDone: (data) => {
          if (data.conversationId) useChatStore.getState().setConversationId(data.conversationId)
          useChatStore.getState().finishStreaming(data.profile || profile, data.queryId)
          if (!startingConversationId && data.conversationId) {
            const title = query.trim().replace(/\s+/g, ' ').slice(0, 72)
            void updateConversation(data.conversationId, { title })
              .catch(() => {})
              .finally(() => void refreshConversations())
          } else {
            void refreshConversations()
          }
          void refreshWorkbench()
        },
        onError: (error) => {
          useChatStore.getState().failStreaming(`${t('common.error')}: ${error}`, profile)
        },
      }, abortRef.current.signal)
    } catch (error) {
      if (error instanceof DOMException && error.name === 'AbortError') {
        useChatStore.getState().failStreaming(t('chat.cancelled'), profile)
        return
      }
      useChatStore.getState().failStreaming(error instanceof Error ? error.message : t('chat.errorStream'), profile)
    }
  }

  const handleNewChat = () => {
    abortRef.current?.abort()
    useChatStore.getState().clearMessages()
  }

  const handleAttach = async (file: File) => {
    setUploading(true)
    useChatStore.getState().setActiveError(null)
    try {
      await uploadDocument(file)
      await refreshWorkbench()
    } catch (error) {
      useChatStore.getState().setActiveError(error instanceof Error ? error.message : t('chat.errorUpload'))
    } finally {
      setUploading(false)
    }
  }

  const handleSaveTitle = async () => {
    if (!conversationId) return

    const title = editTitle.trim()
    if (!title) {
      setError(t('chat.errorEmptyTitle'))
      return
    }

    setSaving(true)
    setError(null)

    try {
      await updateConversation(conversationId, { title })
      setEditingTitle(false)
      void refreshConversations()
    } catch (err) {
      setError(err instanceof Error ? err.message : t('common.error'))
    } finally {
      setSaving(false)
    }
  }

  const handleCancelEdit = () => {
    setEditTitle(activeConversationTitle)
    setEditingTitle(false)
    setError(null)
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      e.preventDefault()
      handleSaveTitle()
    } else if (e.key === 'Escape') {
      e.preventDefault()
      handleCancelEdit()
    }
  }

  return (
    <div className="flex h-full min-h-0 flex-col gap-y-3 overflow-hidden bg-white px-4 pb-6 pt-3 text-slate-950">
      {(activeError || workbenchError) && (
        <div className="mx-auto max-w-[860px] rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
          {activeError || workbenchError}
        </div>
      )}

      <div className="mx-auto flex min-h-0 w-full max-w-[860px] flex-1 flex-col">
        {/* Title header */}
        {!showPreview && (
          <div className="mb-4 flex shrink-0 items-center justify-between gap-4 rounded-lg border border-slate-200 bg-white px-4 py-3 shadow-sm">
            <div className="min-w-0">
              {editingTitle ? (
                <div className="flex items-center gap-2">
                  <input
                    type="text"
                    value={editTitle}
                    onChange={(e) => setEditTitle(e.target.value)}
                    onKeyDown={handleKeyDown}
                    autoFocus
                    className="min-w-0 flex-1 rounded border border-slate-200 px-3 py-2 text-[14px] font-semibold text-slate-950 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
                    placeholder={t('chat.untitled')}
                  />
                  {error && (
                    <span className="text-[12px] text-red-500">{error}</span>
                  )}
                  <button
                    onClick={handleSaveTitle}
                    disabled={saving}
                    className={`ml-2 h-8 shrink-0 rounded-md border border-slate-200 px-3 text-[12px] font-medium text-slate-600 ${saving ? 'bg-slate-50' : 'hover:bg-slate-50'} disabled:opacity-50`}
                  >
                    {saving ? t('common.saving') : t('common.save')}
                  </button>
                  <button
                    onClick={handleCancelEdit}
                    className="ml-2 h-8 shrink-0 rounded-md border border-slate-200 px-3 text-[12px] font-medium text-slate-600 hover:bg-slate-50"
                  >
                    {t('common.cancel')}
                  </button>
                </div>
              ) : (
                <p className="truncate text-[14px] font-semibold text-slate-950 whitespace-nowrap cursor-pointer hover:underline" onClick={() => {
                  setEditTitle(activeConversationTitle)
                  setEditingTitle(true)
                  setError(null)
                }}>
                  {activeConversationTitle}
                </p>
              )}
              <p className="mt-0.5 text-[12px] text-slate-400">
                {conversationId ? t('chat.savedConversation') : t('chat.draftConversation')} · {t('chat.messages', { count: messages.length })}
              </p>
            </div>
            <button
              onClick={handleNewChat}
              className="h-8 shrink-0 rounded-md border border-slate-200 px-3 text-[12px] font-medium text-slate-600 hover:bg-slate-50"
            >
              {t('chat.newChat')}
            </button>
          </div>
        )}

        {showPreview ? (
          <div className="flex flex-col items-center justify-center pt-14">
            <div className="mb-8 text-center">
              <h1 className="text-[34px] font-medium leading-tight tracking-[-0.015em] text-slate-950">
                {t('chat.title')}
              </h1>
              <p className="mt-2 text-[16px] leading-6 text-slate-500">
                {t('chat.subtitle')}
              </p>
            </div>

            <ChatInput
              onSend={handleSend}
              onAttach={handleAttach}
              disabled={isStreaming || healthStatus === 'offline'}
              uploading={uploading}
            />

            {suggestedQuestions.length > 0 && (
              <div className="mt-6 flex flex-wrap justify-center gap-2.5">
                {suggestedQuestions.map((prompt) => (
                  <button
                    key={prompt}
                    onClick={() => handleSend(prompt)}
                    className="h-9 shrink-0 whitespace-nowrap rounded-full border border-slate-200 bg-white px-4 text-[13px] font-medium text-blue-600 shadow-sm transition-colors hover:border-blue-200 hover:bg-blue-50"
                  >
                    {prompt}
                  </button>
                ))}
              </div>
            )}

            {workbench && workbench.corpus.documents > 0 && (
              <p className="mt-5 text-center text-xs text-slate-400">
                {t('chat.indexSummary', {
                  documents: workbench.corpus.documents,
                  active: workbench.connectors.active,
                  total: workbench.connectors.total,
                })}
              </p>
            )}
          </div>
        ) : (
          <>
            <div className="min-h-0 flex-1 overflow-y-auto">
              <div className="space-y-5">
                {messages.map((msg) => (
                  <ChatMessage
                    key={msg.id}
                    message={msg}
                    onFeedback={msg.queryId ? ((type) => {
                      submitFeedback(msg.queryId as string, type).catch(() => {})
                    }) : undefined}
                  />
                ))}
                {isStreaming && currentStreamText && (
                  <ChatMessage
                    message={{
                      id: 'streaming',
                      role: 'assistant',
                      content: currentStreamText,
                      sources: currentSources.length > 0 ? currentSources : undefined,
                      confidence: currentConfidence || undefined,
                      timestamp: Date.now(),
                    }}
                    isStreaming
                  />
                )}
                {isStreaming && !currentStreamText && (
                  <div role="status" aria-live="polite" className="flex items-center gap-3 text-slate-500">
                    <LoaderCircle className="h-5 w-5 animate-spin text-slate-400" />
                    <span className="text-[14px]">{t('chat.thinking')}</span>
                  </div>
                )}
                <div ref={bottomRef} />
              </div>
            </div>

            <div className="shrink-0">
              <ChatInput
                onSend={handleSend}
                onAttach={handleAttach}
                disabled={isStreaming || healthStatus === 'offline'}
                uploading={uploading}
              />
            </div>
          </>
        )}
      </div>
    </div>
  )
}
