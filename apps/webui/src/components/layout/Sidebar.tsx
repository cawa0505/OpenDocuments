import { useAppStore } from '../../stores/appStore'
import { useChatStore } from '../../stores/chatStore'
import { useEffect, useMemo, useState } from 'react'
import type { ReactNode } from 'react'
import { deleteConversation, getConversationMessages, listConversations } from '../../lib/api'
import type { ChatMessage, Conversation, ConversationMessage, SearchResult } from '../../lib/types'
import {
  Clock3,
  FileText,
  Folder,
  Link2,
  MessageSquare,
  Plus,
  Search,
  Settings,
  ShieldCheck,
  Trash2,
  Briefcase,
  BookOpen,
} from 'lucide-react'
import { translate as tr, type Locale } from '../../lib/i18n'
import { ConfirmDialog } from '../ui/ConfirmDialog'

type PageId = 'chat' | 'documents' | 'collections' | 'connectors' | 'health' | 'settings' | 'workspaces' | 'dictionary'

const NAV_ITEMS: { id: PageId; labelKey: string; icon: ReactNode }[] = [
  { id: 'chat', labelKey: 'nav.ask', icon: <Search size={19} strokeWidth={2} /> },
  { id: 'documents', labelKey: 'nav.documents', icon: <FileText size={19} strokeWidth={1.9} /> },
  { id: 'collections', labelKey: 'nav.collections', icon: <Folder size={19} strokeWidth={1.9} /> },
  { id: 'connectors', labelKey: 'nav.connections', icon: <Link2 size={19} strokeWidth={1.9} /> },
  { id: 'health', labelKey: 'nav.activity', icon: <Clock3 size={19} strokeWidth={1.9} /> },
]

function NavButton({ item, active, locale, onClick }: { item: { id: PageId; labelKey: string; icon: ReactNode }; active: boolean; locale: Locale; onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      className={`flex h-11 w-full items-center gap-3 rounded-lg px-4 text-[14px] font-medium transition-colors ${
        active
          ? 'bg-blue-50 text-blue-600'
          : 'text-slate-600 hover:bg-slate-50 hover:text-slate-900'
      }`}
    >
      <span className={active ? 'text-blue-600' : 'text-slate-500'}>{item.icon}</span>
      {tr(locale, item.labelKey)}
    </button>
  )
}

function LogoMark() {
  return (
    <div className="flex h-8 w-8 items-center justify-center rounded-lg border border-blue-100 bg-white text-blue-600 shadow-sm">
      <ShieldCheck size={22} strokeWidth={2.2} />
    </div>
  )
}

function getConversationTimestamp(conversation: Conversation, field: 'updated' | 'created') {
  const value = field === 'updated'
    ? conversation.updatedAt || conversation.updated_at || conversation.createdAt || conversation.created_at
    : conversation.createdAt || conversation.created_at || conversation.updatedAt || conversation.updated_at
  return value ? new Date(value).getTime() : 0
}

function formatConversationDate(conversation: Conversation, locale: Locale) {
  const value = conversation.updatedAt || conversation.updated_at || conversation.createdAt || conversation.created_at
  if (!value) return ''
  return new Date(value).toLocaleDateString(locale === 'ko' ? 'ko-KR' : 'en-US', { month: 'short', day: 'numeric' })
}

function parseSources(value: ConversationMessage['sources']): SearchResult[] | undefined {
  if (!value) return undefined
  if (Array.isArray(value)) return value
  try {
    const parsed: unknown = JSON.parse(value)
    if (Array.isArray(parsed)) return parsed as SearchResult[]
  } catch {}
  return undefined
}

function toChatMessage(message: ConversationMessage): ChatMessage {
  const confidenceScore = message.confidenceScore ?? message.confidence_score
  const createdAt = message.createdAt || message.created_at
  return {
    id: message.id,
    role: message.role,
    content: message.content,
    sources: parseSources(message.sources),
    confidence: confidenceScore === null || confidenceScore === undefined
      ? undefined
      : { score: confidenceScore, level: confidenceScore >= 0.75 ? 'high' : confidenceScore >= 0.45 ? 'medium' : 'low', reason: 'Restored from saved conversation' },
    profile: message.profileUsed || message.profile_used,
    timestamp: createdAt ? new Date(createdAt).getTime() : Date.now(),
  }
}

export function Sidebar() {
  const { currentPage, setPage, locale } = useAppStore()
  const t = (key: string, values?: Record<string, string | number>) => tr(locale, key, values)
  const {
    conversations,
    conversationsLoading,
    conversationId,
    conversationSort,
    clearMessages,
    setActiveError,
    setConversationId,
    setConversationSort,
    setConversations,
    setConversationsLoading,
    setMessages,
  } = useChatStore()
  const [loadingConversationId, setLoadingConversationId] = useState<string | null>(null)
  const [deletingConversationId, setDeletingConversationId] = useState<string | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<Conversation | null>(null)

  const sortedConversations = useMemo(() => {
    return [...conversations].sort((a, b) => {
      if (conversationSort === 'title') {
        return (a.title || t('chat.untitled')).localeCompare(b.title || t('chat.untitled'))
      }
      return getConversationTimestamp(b, conversationSort) - getConversationTimestamp(a, conversationSort)
    })
  }, [conversationSort, conversations, locale])

  const refreshConversations = async () => {
    setConversationsLoading(true)
    try {
      const result = await listConversations({ limit: 80 })
      setConversations(result.conversations)
    } catch (error) {
      setActiveError(error instanceof Error ? error.message : t('chat.errorSessions'))
    } finally {
      setConversationsLoading(false)
    }
  }

  useEffect(() => {
    void refreshConversations()
  }, [])

  const handleNewChat = () => {
    clearMessages()
    setPage('chat')
  }

  const handleOpenConversation = async (conversation: Conversation) => {
    setLoadingConversationId(conversation.id)
    setActiveError(null)
    try {
      const result = await getConversationMessages(conversation.id)
      setMessages(result.messages.map(toChatMessage))
      setConversationId(conversation.id)
      setPage('chat')
    } catch (error) {
      setActiveError(error instanceof Error ? error.message : t('chat.errorSessions'))
    } finally {
      setLoadingConversationId(null)
    }
  }

  const handleDeleteConversation = (conversation: Conversation) => {
    setDeleteTarget(conversation)
  }

  const confirmDeleteConversation = async () => {
    if (!deleteTarget) return
    setDeletingConversationId(deleteTarget.id)
    setActiveError(null)
    try {
      await deleteConversation(deleteTarget.id)
      if (conversationId === deleteTarget.id) clearMessages()
      await refreshConversations()
      setDeleteTarget(null)
    } catch (error) {
      setActiveError(error instanceof Error ? error.message : t('chat.errorSessions'))
    } finally {
      setDeletingConversationId(null)
    }
  }

  return (
    <aside className="flex h-screen w-[264px] shrink-0 flex-col border-r border-slate-200 bg-white">
      <div className="flex h-[88px] flex-col justify-center px-5 border-b border-slate-100 pb-3 mt-2">
        <div className="flex items-center gap-3">
          <LogoMark />
          <h1 className="text-[21px] font-semibold tracking-[-0.01em] text-slate-950">OpenDocuments</h1>
        </div>
        <div className="mt-2 flex items-center gap-1.5">
          <span className="inline-flex items-center gap-1 rounded-md bg-blue-50 px-2 py-0.5 text-[11.5px] font-semibold text-blue-700 ring-1 ring-inset ring-blue-700/10">
            <span className="h-1.5 w-1.5 rounded-full bg-blue-500" />
            Workspace: {localStorage.getItem('active-workspace') || 'default'}
          </span>
        </div>
      </div>

      <nav className="px-2 pt-3">
        <button
          onClick={handleNewChat}
          className="mb-2 flex h-10 w-full items-center justify-center gap-2 rounded-lg bg-blue-600 px-4 text-[14px] font-medium text-white shadow-sm hover:bg-blue-700"
        >
          <Plus size={16} strokeWidth={2} />
          {t('nav.newChat')}
        </button>
        <div className="space-y-1">
          {NAV_ITEMS.map((item) => (
            <NavButton
              key={item.id}
              item={item}
              locale={locale}
              active={currentPage === item.id}
              onClick={() => setPage(item.id)}
            />
          ))}
        </div>
      </nav>

      <div className="mt-8 min-h-0 flex-1 px-5">
        <div className="mb-3 flex items-center justify-between gap-2">
          <p className="text-[11px] font-semibold uppercase tracking-wide text-slate-500">{t('nav.recent')}</p>
          <select
            value={conversationSort}
            onChange={(event) => setConversationSort(event.target.value as 'updated' | 'created' | 'title')}
            className="h-7 rounded-md border border-slate-200 bg-white px-2 text-[11px] font-medium text-slate-500 outline-none"
            aria-label={t('nav.recent')}
          >
            <option value="updated">{t('nav.sort.updated')}</option>
            <option value="created">{t('nav.sort.created')}</option>
            <option value="title">{t('nav.sort.title')}</option>
          </select>
        </div>
        {conversationsLoading ? (
          <p className="text-[13px] leading-5 text-slate-400">{t('nav.loadingConversations')}</p>
        ) : sortedConversations.length > 0 ? (
          <div className="max-h-[calc(100vh-430px)] space-y-1 overflow-auto pr-1">
            {sortedConversations.map((conversation) => {
              const active = conversation.id === conversationId
              const busy = loadingConversationId === conversation.id || deletingConversationId === conversation.id
              return (
                <div
                  key={conversation.id}
                  className={`group flex items-center gap-2 rounded-md px-2 py-2 ${
                    active ? 'bg-blue-50' : 'hover:bg-slate-50'
                  }`}
                >
                  <button
                    onClick={() => void handleOpenConversation(conversation)}
                    disabled={busy}
                    className="flex min-w-0 flex-1 items-start gap-2 text-left disabled:opacity-50"
                  >
                    <MessageSquare size={14} strokeWidth={1.9} className={`mt-0.5 shrink-0 ${active ? 'text-blue-600' : 'text-slate-500'}`} />
                    <span className="min-w-0 flex-1">
                      <span className={`block truncate text-[13px] font-medium ${active ? 'text-blue-700' : 'text-slate-700'}`}>
                        {conversation.title || t('chat.untitled')}
                      </span>
                      <span className="mt-0.5 block truncate text-[11px] text-slate-400">
                        {busy ? t('common.loading') : formatConversationDate(conversation, locale)}
                      </span>
                    </span>
                  </button>
                  <button
                    onClick={() => void handleDeleteConversation(conversation)}
                    disabled={busy}
                    className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-slate-300 opacity-0 hover:bg-red-50 hover:text-red-600 group-hover:opacity-100 disabled:opacity-30"
                    aria-label={`${t('common.delete')} ${conversation.title || t('chat.untitled')}`}
                  >
                    <Trash2 size={13} />
                  </button>
                </div>
              )
            })}
          </div>
        ) : (
          <p className="text-[13px] leading-5 text-slate-400">{t('nav.noConversations')}</p>
        )}
      </div>

      <div className="mt-auto px-5 pb-6 space-y-2">
        <button
          onClick={() => setPage('workspaces')}
          className={`flex h-10 w-full items-center gap-3 text-left text-[15px] font-medium ${
            currentPage === 'workspaces' ? 'text-blue-600' : 'text-slate-600 hover:text-slate-950'
          }`}
        >
          <Briefcase size={20} strokeWidth={1.9} />
          {t('workspaces.title') || 'Workspaces'}
        </button>
        <button
          onClick={() => setPage('dictionary')}
          className={`flex h-10 w-full items-center gap-3 text-left text-[15px] font-medium ${
            currentPage === 'dictionary' ? 'text-blue-600' : 'text-slate-600 hover:text-slate-950'
          }`}
        >
          <BookOpen size={20} strokeWidth={1.9} />
          {t('settings.glossary') || 'Glossary'}
        </button>
        <button
          onClick={() => setPage('settings')}
          className={`flex h-10 w-full items-center gap-3 text-left text-[15px] font-medium ${
            currentPage === 'settings' ? 'text-blue-600' : 'text-slate-600 hover:text-slate-950'
          }`}
        >
          <Settings size={20} strokeWidth={1.9} />
          {t('nav.settings')}
        </button>
      </div>
      <ConfirmDialog
        open={deleteTarget !== null}
        title={t('common.delete')}
        description={deleteTarget ? `${t('common.delete')} "${deleteTarget.title || t('chat.untitled')}"?` : undefined}
        confirmLabel={t('common.delete')}
        cancelLabel={t('common.cancel')}
        busyLabel={t('common.deleting')}
        busy={deletingConversationId !== null}
        danger
        onConfirm={() => void confirmDeleteConversation()}
        onCancel={() => setDeleteTarget(null)}
      />
    </aside>
  )
}
