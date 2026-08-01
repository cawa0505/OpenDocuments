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

  setTheme: (theme: Theme) => void
  setLocale: (locale: Locale) => void
  setProfile: (profile: RAGProfile) => void
  setPage: (page: Page) => void
  toggleSidebar: () => void
}

function getEffectiveTheme(theme: Theme): 'light' | 'dark' {
  if (theme === 'system') {
    return typeof window !== 'undefined' && window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
  }
  return theme
}

const initialTheme = (typeof localStorage !== 'undefined' ? localStorage.getItem('opendocuments-theme') as Theme : 'light') || 'light'
const initialEffective = getEffectiveTheme(initialTheme)

if (typeof document !== 'undefined') {
  document.documentElement.classList.toggle('dark', initialEffective === 'dark')
}

export const useAppStore = create<AppState>((set) => ({
  theme: initialTheme,
  effectiveTheme: initialEffective,
  locale: (typeof localStorage !== 'undefined' ? localStorage.getItem('opendocuments-locale') : null)
    ? normalizeLocale(localStorage.getItem('opendocuments-locale'))
    : detectLocale(),
  profile: ((typeof localStorage !== 'undefined' ? localStorage.getItem('opendocuments-profile') as RAGProfile : 'fast') || 'fast'),
  currentPage: 'chat',
  sidebarOpen: true,

  setTheme: (theme) => {
    localStorage.setItem('opendocuments-theme', theme)
    const effective = getEffectiveTheme(theme)
    document.documentElement.classList.toggle('dark', effective === 'dark')
    set({ theme, effectiveTheme: effective })
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
}))
