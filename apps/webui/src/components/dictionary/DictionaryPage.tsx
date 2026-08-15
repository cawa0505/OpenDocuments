import React, { useState, useEffect } from 'react'
import { useAppStore } from '../../stores/appStore.js'
import { translate } from '../../lib/i18n.js'
import { 
  getDictionary, 
  addDictionaryEntry, 
  deleteDictionaryEntry, 
  importDictionarySeed 
} from '../../lib/api.js'
import { ConfirmDialog } from '../ui/ConfirmDialog.js'

interface DictionaryEntry {
  id: string
  key: string
  value: string
}

export default function DictionaryPage() {
  const { locale, workspaceName } = useAppStore()
  const activeWorkspace = workspaceName || ''
  const [entries, setEntries] = useState<DictionaryEntry[]>([])
  const [key, setKey] = useState('')
  const [value, setValue] = useState('')
  const [isLoading, setIsLoading] = useState(true)
  const [isSubmitting, setIsSubmitting] = useState(false)
  const [isImporting, setIsImporting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null)
  const [seedConfirm, setSeedConfirm] = useState(false)

  // 局部高雅多語系包裝函數
  const tr = (k: string, values?: Record<string, string | number>) => translate(locale, k, values)

  useEffect(() => {
    fetchEntries()
  }, [activeWorkspace])

  const fetchEntries = async () => {
    setIsLoading(true)
    setError(null)
    try {
      const res = await getDictionary()
      setEntries(res.entries || [])
    } catch (err: any) {
      setError(err.message || 'Failed to fetch glossary entries')
    } finally {
      setIsLoading(false)
    }
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    const trimmedKey = key.trim()
    const trimmedVal = value.trim()
    
    if (!trimmedKey || !trimmedVal) return
    
    setIsSubmitting(true)
    setError(null)
    try {
      const newEntry = await addDictionaryEntry(trimmedKey, trimmedVal)
      setEntries([newEntry, ...entries])
      setKey('')
      setValue('')
    } catch (err: any) {
      setError(err.message || 'Failed to save terminology entry')
    } finally {
      setIsSubmitting(false)
    }
  }

  const handleDelete = (id: string) => {
    setDeleteTarget(id)
  }

  const confirmDelete = async () => {
    if (!deleteTarget) return
    setError(null)
    try {
      await deleteDictionaryEntry(deleteTarget)
      setEntries(entries.filter(e => e.id !== deleteTarget))
      setDeleteTarget(null)
    } catch (err: any) {
      setError(err.message || 'Failed to delete entry')
    }
  }

  const handleImportSeed = () => {
    setSeedConfirm(true)
  }

  const confirmImportSeed = async () => {
    setSeedConfirm(false)
    setIsImporting(true)
    setError(null)
    try {
      await importDictionarySeed('zh-TW')
      await fetchEntries() // 重新載入列表
    } catch (err: any) {
      setError(err.message || 'Failed to import glossary seed')
    } finally {
      setIsImporting(false)
    }
  }

  return (
    <>
      <div className="min-h-full bg-slate-50 px-6 py-6 text-slate-950">
      <div className="mx-auto max-w-6xl space-y-5">
        {/* 標題大盤區 */}
        <header className="flex items-start justify-between gap-4">
          <div>
            <p className="text-[13px] font-medium text-blue-600">{tr('settings.glossary.eyebrow')}</p>
            <h2 className="mt-1 text-[26px] font-semibold tracking-normal">{tr('settings.glossary')}</h2>
            <p className="mt-2 max-w-2xl text-[14px] leading-6 text-slate-500">
              {tr('settings.glossary.desc')}
            </p>
          </div>
          <div className="flex h-9 items-center gap-2 rounded-md border border-blue-100 bg-blue-50/50 px-3 text-[13px] font-medium text-blue-700 shadow-sm shrink-0 whitespace-nowrap">
            <svg className="w-4 h-4 text-blue-600" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10" />
            </svg>
            <span>Workspace: {activeWorkspace}</span>
          </div>
        </header>

      {/* 簡潔的 Slate 灰色引導卡片 */}
      <div className="bg-slate-50 border border-slate-200/80 rounded-xl p-5 text-xs text-slate-700 leading-relaxed flex flex-col md:flex-row items-start md:items-center justify-between gap-5 shadow-sm">
        <div className="space-y-1 md:max-w-2xl">
          <strong className="text-sm block md:inline font-bold text-slate-900">
            {tr('settings.glossary.helpTitle')}
          </strong>
          <p className="mt-1 text-slate-600 leading-relaxed">
            {tr('settings.glossary.helpDesc')}
          </p>
        </div>
        <button
          onClick={handleImportSeed}
          disabled={isImporting}
          className="w-full md:w-auto shrink-0 flex items-center justify-center space-x-1.5 bg-blue-600 hover:bg-blue-700 active:bg-blue-800 text-white font-semibold text-xs px-4 py-2.5 rounded-lg transition-all disabled:opacity-50 shadow-sm whitespace-nowrap"
        >
          {isImporting && (
            <svg className="animate-spin h-3 w-3 text-white" fill="none" viewBox="0 0 24 24">
              <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
              <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
            </svg>
          )}
          <span>{isImporting ? tr('settings.glossary.importing') : tr('settings.glossary.seedBtn')}</span>
        </button>
      </div>

      {/* 錯誤資訊展示區 (防止跑版，附帶圓角與動畫) */}
      {error && (
        <div className="rounded-xl border border-red-200 bg-red-50/60 p-4 text-xs text-red-700 flex items-start space-x-2.5 animate-fadeIn">
          <svg className="w-4 h-4 text-red-500 shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
          </svg>
          <div className="flex-1 font-medium">{error}</div>
        </div>
      )}

      {/* 新增詞彙表單 */}
      <form onSubmit={handleSubmit} className="bg-white border border-slate-200/80 rounded-xl p-5 space-y-4 shadow-sm">
        <h3 className="text-sm font-semibold text-slate-900 flex items-center space-x-1.5">
          <svg className="w-4 h-4 text-slate-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M12 4v16m8-8H4" />
          </svg>
          <span>{tr('settings.glossary.add')}</span>
        </h3>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div className="space-y-1.5">
            <label className="text-xs text-slate-500 font-semibold">{tr('settings.glossary.keyLabel')}</label>
            <input
              type="text"
              required
              value={key}
              onChange={e => setKey(e.target.value)}
              placeholder={tr('settings.glossary.keyPlaceholder')}
              className="w-full text-sm bg-slate-50/50 border border-slate-200 focus:border-blue-500 focus:bg-white focus:ring-1 focus:ring-blue-500 rounded-lg px-3 py-2 text-slate-900 placeholder-slate-400 focus:outline-none transition-all"
            />
          </div>
          <div className="space-y-1.5">
            <label className="text-xs text-slate-500 font-semibold">{tr('settings.glossary.valueLabel')}</label>
            <input
              type="text"
              required
              value={value}
              onChange={e => setValue(e.target.value)}
              placeholder={tr('settings.glossary.valuePlaceholder')}
              className="w-full text-sm bg-slate-50/50 border border-slate-200 focus:border-blue-500 focus:bg-white focus:ring-1 focus:ring-blue-500 rounded-lg px-3 py-2 text-slate-900 placeholder-slate-400 focus:outline-none transition-all"
            />
          </div>
        </div>
        <div className="flex justify-end pt-1">
          <button
            type="submit"
            disabled={isSubmitting}
            className="w-full sm:w-auto flex items-center justify-center space-x-1.5 bg-slate-900 hover:bg-slate-800 active:bg-black text-white px-5 py-2 rounded-lg text-sm font-semibold transition-all disabled:opacity-50 shadow-sm whitespace-nowrap"
          >
            {isSubmitting && (
              <svg className="animate-spin h-3.5 w-3.5 text-white" fill="none" viewBox="0 0 24 24">
                <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
              </svg>
            )}
            <span>{isSubmitting ? tr('settings.glossary.saving') : tr('settings.glossary.add')}</span>
          </button>
        </div>
      </form>

      {/* 詞彙列表區 */}
      <div className="bg-white border border-slate-200/80 rounded-xl overflow-hidden shadow-sm">
        {isLoading ? (
          <div className="py-16 text-center text-sm text-slate-500 flex flex-col items-center justify-center space-y-2">
            <svg className="animate-spin h-5 w-5 text-slate-400" fill="none" viewBox="0 0 24 24">
              <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
              <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
            </svg>
            <span>Loading glossary...</span>
          </div>
        ) : entries.length === 0 ? (
          // 科技感虛線空狀態引導
          <div className="py-16 text-center px-6 flex flex-col items-center justify-center max-w-md mx-auto space-y-4">
            <div className="p-3 bg-slate-50 rounded-2xl border border-dashed border-slate-200 text-slate-400">
              <svg className="w-8 h-8" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M12 6.042A8.967 8.967 0 006 3.75c-1.052 0-2.062.18-3 .512v14.25A8.987 8.967 0 016 18c2.305 0 4.408.867 6 2.292m0-14.25a8.966 8.967 0 016-2.292c1.052 0 2.062.18 3 .512v14.25A8.987 8.967 0 0018 18a8.967 8.967 0 00-6 2.292m0-14.25v14.25" />
              </svg>
            </div>
            <div className="space-y-1">
              <h4 className="text-sm font-semibold text-slate-900">Empty Terminology</h4>
              <p className="text-xs text-slate-500 leading-relaxed">
                {tr('settings.glossary.noTerms')}
              </p>
            </div>
          </div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-left border-collapse text-sm">
              <thead>
                <tr className="border-b border-slate-100 bg-slate-50/50 text-xs text-slate-500 font-medium">
                  <th className="px-6 py-3.5">{tr('settings.glossary.alignText')}</th>
                  <th className="px-6 py-3.5">{tr('settings.glossary.alignedTo')}</th>
                  <th className="px-6 py-3.5 w-24 text-right">{tr('settings.glossary.actions')}</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-100">
                {entries.map(entry => (
                  <tr key={entry.id} className="hover:bg-slate-50/30 transition-colors">
                    <td className="px-6 py-4 font-semibold text-slate-900">{entry.key}</td>
                    <td className="px-6 py-4 text-slate-600">
                      <code className="bg-slate-50 text-blue-600 border border-slate-100 px-2 py-0.5 rounded text-xs font-mono">
                        {entry.value}
                      </code>
                    </td>
                    <td className="px-6 py-4 text-right">
                      <button
                        onClick={() => handleDelete(entry.id)}
                        className="text-red-600 hover:text-red-700 font-semibold text-xs transition-colors"
                      >
                        {tr('settings.glossary.delete')}
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  </div>
  <ConfirmDialog
    open={deleteTarget !== null}
    title={tr('common.delete')}
    description={tr('settings.glossary.deleteConfirm')}
    confirmLabel={tr('common.delete')}
    cancelLabel={tr('common.cancel')}
    danger
    onConfirm={() => void confirmDelete()}
    onCancel={() => setDeleteTarget(null)}
  />
  <ConfirmDialog
    open={seedConfirm}
    title={tr('settings.glossary.seedBtn')}
    description={tr('settings.glossary.seedConfirm')}
    confirmLabel={tr('settings.glossary.seedBtn')}
    cancelLabel={tr('common.cancel')}
    busy={isImporting}
    busyLabel={tr('settings.glossary.importing')}
    onConfirm={() => void confirmImportSeed()}
    onCancel={() => setSeedConfirm(false)}
  />
  </>
  )
}