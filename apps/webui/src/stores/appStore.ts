import { create } from 'zustand'
import type { RAGProfile } from '../lib/types'
import { detectLocale, normalizeLocale, type Locale } from '../lib/i18n'

export type Theme = 'light' | 'dark' | 'system'
export type Page = 'dashboard' | 'chat' | 'documents' | 'collections' | 'settings' | 'health' | 'connectors' | 'plugins' | 'workspaces' | 'dictionary'

interface AppState {
  theme: Theme
  effectiveTheme: 'light' | 'dark'
  locale: Locale
  profile: RAGProfile
  currentPage: Page
  sidebarOpen: boolean
  workspaceName: string

  setTheme: (theme: Theme) => void
  setLocale: (locale: Locale) => void
  setProfile: (profile: RAGProfile) => void
  setPage: (page: Page) => void
  toggleSidebar: () => void
  setWorkspaceName: (name: string) => void
}

// ponytail: Light Mode 固定，任何切換路徑皆無效（dark mode 未完成，hide 後不再提供）
const initialTheme: Theme = 'light'
const initialEffective: 'light' | 'dark' = 'light'

if (typeof document !== 'undefined') {
  document.documentElement.classList.remove('dark')
}

const RAG_PROFILES: readonly RAGProfile[] = ['fast', 'balanced', 'precise']

// 檢索偏好單一狀態源（spec: webui-ux-hardening）：僅此 store slice 為唯一來源，
// localStorage 僅作為持久化載體；啟動時驗證，避免持久化髒值形成第二狀態源。
function readStoredProfile(): RAGProfile {
  if (typeof localStorage === 'undefined') return 'fast'
  const stored = localStorage.getItem('opendocuments-profile')
  return RAG_PROFILES.includes(stored as RAGProfile) ? (stored as RAGProfile) : 'fast'
}

export const useAppStore = create<AppState>((set) => ({
  theme: initialTheme,
  effectiveTheme: initialEffective,
  locale: (typeof localStorage !== 'undefined' ? localStorage.getItem('opendocuments-locale') : null)
    ? normalizeLocale(localStorage.getItem('opendocuments-locale'))
    : detectLocale(),
  profile: readStoredProfile(),
  currentPage: 'chat',
  sidebarOpen: true,
  workspaceName: '',

  setTheme: () => {
    // ponytail: Light Mode 固定；保留簽名避免破壞呼叫端，但一律 light
    localStorage.setItem('opendocuments-theme', 'light')
    document.documentElement.classList.remove('dark')
    set({ theme: 'light', effectiveTheme: 'light' })
  },

  setLocale: (locale) => {
    localStorage.setItem('opendocuments-locale', locale)
    document.documentElement.lang = locale
    set({ locale })
  },

  setProfile: (profile) => {
    localStorage.setItem('opendocuments-profile', profile)
    set({ profile })
  },

  setPage: (page) => set({ currentPage: page }),
  toggleSidebar: () => set((s) => ({ sidebarOpen: !s.sidebarOpen })),
  setWorkspaceName: (name) => set({ workspaceName: name }),
}))
