import { create } from 'zustand'
import type { ChatMessage, SearchResult, ConfidenceResult, Conversation } from '../lib/types'

interface ChatState {
  messages: ChatMessage[]
  conversations: Conversation[]
  conversationsLoading: boolean
  isStreaming: boolean
  currentStreamText: string
  currentSources: SearchResult[]
  currentConfidence: ConfidenceResult | null
  conversationId: string | null
  activeError: string | null
  /** 最近一次發送的提問文字；retry 必須重發同一則，cancel 不重發（spec: webui-ux-hardening） */
  lastQuery: string | null
  conversationSort: 'updated' | 'created' | 'title'

  addUserMessage: (content: string) => void
  setLastQuery: (query: string | null) => void
  dropFailedTurn: () => void
  setMessages: (messages: ChatMessage[]) => void
  setConversations: (conversations: Conversation[]) => void
  setConversationsLoading: (loading: boolean) => void
  startStreaming: () => void
  appendStreamChunk: (text: string) => void
  setSources: (sources: SearchResult[]) => void
  setConfidence: (confidence: ConfidenceResult) => void
  finishStreaming: (profile: string, queryId?: string) => void
  failStreaming: (message: string, profile: string) => void
  setConversationId: (id: string | null) => void
  setConversationSort: (sort: 'updated' | 'created' | 'title') => void
  clearMessages: () => void
  setActiveError: (message: string | null) => void
  /** 重新掛載零事件逾時計時器；任何事件到達時必須重掛，逾時時觸發 onTimeout（spec: webui-ux-hardening） */
  armStreamTimeout: (onTimeout: () => void, windowMs?: number) => void
  /** 清除逾時計時器（串流結束/取消時必須呼叫，單一出口） */
  clearStreamTimeout: () => void
}
let messageCounter = 0

// 零事件窗口逾時計時器單一來源（spec: webui-ux-hardening）：
// 模組內單一 handle，任何事件到達重掛、結束/取消即清除。
const STREAM_IDLE_WINDOW_MS = 60_000
let streamIdleTimer: ReturnType<typeof setTimeout> | null = null

export const useChatStore = create<ChatState>((set, get) => ({
  messages: [],
  conversations: [],
  conversationsLoading: false,
  isStreaming: false,
  currentStreamText: '',
  currentSources: [],
  currentConfidence: null,
  conversationId: null,
  activeError: null,
  conversationSort: 'updated',
  lastQuery: null,

  addUserMessage: (content) => {
    set((s) => ({
      messages: [...s.messages, {
        id: `msg-${++messageCounter}`,
        role: 'user',
        content,
        timestamp: Date.now(),
      }],
    }))
  },

  setLastQuery: (query) => set({ lastQuery: query }),

  setMessages: (messages) => {
    // 恢復會話：等待路徑結束，必須清掉逾時計時器（spec: webui-ux-hardening）
    if (streamIdleTimer !== null) { clearTimeout(streamIdleTimer); streamIdleTimer = null }
    set({ messages, currentStreamText: '', currentSources: [], currentConfidence: null, isStreaming: false })
  },

  setConversations: (conversations) => set({ conversations }),

  setConversationsLoading: (loading) => set({ conversationsLoading: loading }),

  startStreaming: () => {
    if (streamIdleTimer !== null) { clearTimeout(streamIdleTimer); streamIdleTimer = null }
    set({
      isStreaming: true,
      currentStreamText: '',
      currentSources: [],
      currentConfidence: null,
      activeError: null,
    })
  },

  // 每個串流事件都重置零事件窗口；逾時觸發由 ChatPage 掛載的回呼處理
  appendStreamChunk: (text) => set((s) => ({
    currentStreamText: s.currentStreamText + text,
  })),

  setSources: (sources) => set({ currentSources: sources }),

  setConfidence: (confidence) => set({ currentConfidence: confidence }),

  armStreamTimeout: (onTimeout, windowMs) => {
    if (streamIdleTimer !== null) clearTimeout(streamIdleTimer)
    streamIdleTimer = setTimeout(() => {
      streamIdleTimer = null
      if (get().isStreaming) onTimeout()
    }, windowMs ?? STREAM_IDLE_WINDOW_MS)
  },

  clearStreamTimeout: () => {
    if (streamIdleTimer !== null) {
      clearTimeout(streamIdleTimer)
      streamIdleTimer = null
    }
  },

  finishStreaming: (profile, queryId?) => {
    const state = get()
    if (streamIdleTimer !== null) { clearTimeout(streamIdleTimer); streamIdleTimer = null }
    set((s) => ({
      messages: [...s.messages, {
        id: `msg-${++messageCounter}`,
        role: 'assistant',
        content: state.currentStreamText,
        sources: state.currentSources,
        confidence: state.currentConfidence || undefined,
        profile,
        queryId,
        timestamp: Date.now(),
      }],
      isStreaming: false,
      currentStreamText: '',
      currentSources: [],
      currentConfidence: null,
    }))
  },

  failStreaming: (message, profile) => {
    const state = get()
    if (streamIdleTimer !== null) { clearTimeout(streamIdleTimer); streamIdleTimer = null }
    set((s) => ({
      messages: [...s.messages, {
        id: `msg-${++messageCounter}`,
        role: 'assistant',
        content: state.currentStreamText || message,
        sources: state.currentSources,
        confidence: state.currentConfidence || undefined,
        profile,
        timestamp: Date.now(),
        error: true,
      }],
      isStreaming: false,
      currentStreamText: '',
      currentSources: [],
      currentConfidence: null,
    }))
  },

  dropFailedTurn: () => set((s) => {
    // retry 前移除上一次失敗產生的錯誤佔位回覆；非錯誤訊息一律不動（單一出口）
    const last = s.messages[s.messages.length - 1]
    if (!last || last.role !== 'assistant' || !last.error) return {}
    return { messages: s.messages.slice(0, -1), activeError: null }
  }),

  setConversationId: (id) => set({ conversationId: id }),

  setConversationSort: (sort) => set({ conversationSort: sort }),

  clearMessages: () => {
    if (streamIdleTimer !== null) { clearTimeout(streamIdleTimer); streamIdleTimer = null }
    set({
      messages: [],
      isStreaming: false,
      currentStreamText: '',
      currentSources: [],
      currentConfidence: null,
      conversationId: null,
      activeError: null,
      lastQuery: null,
    })
  },

  setActiveError: (message) => set({ activeError: message }),
}))
