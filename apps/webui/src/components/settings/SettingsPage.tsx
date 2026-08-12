import { useEffect, useState } from 'react'
import type { ReactNode, FormEvent } from 'react'
import { RefreshCw, AlertCircle, Copy, Check, Trash2, Key, Play, Plus, Power, ToggleLeft, X, Loader2 } from 'lucide-react'
import { getHealth, getWorkbench, checkVersion, listLlmProviders, upsertLlmProvider, deleteLlmProvider, testLlmProvider } from '../../lib/api'
import type { VersionCheckResponse } from '../../lib/api'
import { useAppStore } from '../../stores/appStore'
import type { RAGProfile, WorkbenchResponse, LlmProvider } from '../../lib/types'
import { translate as tr } from '../../lib/i18n'

function SettingCard({ title, description, children, className }: { title: string; description?: string; children: ReactNode; className?: string }) {
  return (
    <section className={`rounded-lg border border-slate-200 bg-white p-5 shadow-sm ${className ?? ''}`}>
      <h3 className="text-[15px] font-semibold text-slate-950">{title}</h3>
      {description && <p className="mt-1 text-[13px] leading-5 text-slate-500">{description}</p>}
      <div className="mt-4">{children}</div>
    </section>
  )
}

function Field({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="rounded-md border border-slate-200 bg-slate-50 px-3 py-2">
      <p className="text-[11px] font-semibold uppercase tracking-wide text-slate-400">{label}</p>
      <div className="mt-1 break-words text-[13px] text-slate-800">{value}</div>
    </div>
  )
}

export function SettingsPage() {
  const { profile, setProfile, locale, setLocale } = useAppStore()
  const t = (key: string, values?: Record<string, string | number>) => tr(locale, key, values)
  const [health, setHealth] = useState<{ status: string; version: string } | null>(null)
  const [workbench, setWorkbench] = useState<WorkbenchResponse | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  
  // 版本更新相關狀態
  const [versionData, setVersionData] = useState<VersionCheckResponse | null>(null)
  const [checkingVersion, setCheckingVersion] = useState(false)
  const [copied, setCopied] = useState(false)

  const refresh = async () => {
    setLoading(true)
    setError(null)
    setCheckingVersion(true)
    
    // 1. 立即異步獲取基本健康與工作台狀態（10ms 極速返回）
    try {
      const [nextHealth, nextWorkbench, nextVersion] = await Promise.all([
        getHealth(),
        getWorkbench(),
        checkVersion().catch(() => null)
      ])
      setHealth(nextHealth)
      setWorkbench(nextWorkbench)
      setVersionData(nextVersion)
    } catch (err) {
      setError(err instanceof Error ? err.message : t('settings.subtitle'))
    } finally {
      setLoading(false)
      setCheckingVersion(false)
    }
  }

  // LLM Provider 管理狀態
  const [llmProviders, setLlmProviders] = useState<LlmProvider[]>([])
  const [llmLoading, setLlmLoading] = useState(false)
  const [llmError, setLlmError] = useState<string | null>(null)
  const [testLoadingId, setTestLoadingId] = useState<string | null>(null)
  const [deleteConfirmId, setDeleteConfirmId] = useState<string | null>(null)
  const [isAddingProvider, setIsAddingProvider] = useState(false)
  const [editingProvider, setEditingProvider] = useState<LlmProvider | null>(null)
  const [formData, setFormData] = useState({
    name: '',
    provider: 'openai',
    baseUrl: '',
    model: '',
    apiKey: '',
    isActive: false
  })

  const loadLlmProviders = async () => {
    setLlmLoading(true)
    setLlmError(null)
    try {
      const response = await listLlmProviders()
      setLlmProviders(response.providers)
    } catch (err) {
      setLlmError(err instanceof Error ? err.message : t('common.unknownError'))
    } finally {
      setLlmLoading(false)
    }
  }

  const handleTestProvider = async (provider: LlmProvider) => {
    setTestLoadingId(provider.id || null)
    try {
      const result = await testLlmProvider({
        providerId: provider.id || '',
        baseUrl: provider.baseUrl,
        model: provider.model,
        apiKey: provider.apiKey
      })
      if (result.ok) {
        // 測試成功，延遲一下顯示結果
        setTimeout(() => {
          alert(`${t('common.testSuccess')} ${result.reply} (${result.latencyMs}ms)`)
        }, 300)
      } else {
        alert(`${t('common.testFailed')}: ${result.error}`)
      }
    } catch (err) {
      alert(`${t('common.testFailed')}: ${err instanceof Error ? err.message : t('common.unknownError')}`)
    } finally {
      setTestLoadingId(null)
    }
  }

  const handleDeleteProvider = async (id: string) => {
    if (deleteConfirmId !== id) {
      setDeleteConfirmId(id)
      setTimeout(() => setDeleteConfirmId(null), 3000)
      return
    }
    try {
      await deleteLlmProvider(id)
      await loadLlmProviders()
      alert(t('common.deleteSuccess'))
    } catch (err) {
      alert(`${t('common.deleteFailed')}: ${err instanceof Error ? err.message : t('common.unknownError')}`)
    } finally {
      setDeleteConfirmId(null)
    }
  }

  const handleToggleActive = async (provider: LlmProvider) => {
    try {
      const updated = { ...provider, isActive: !provider.isActive }
      await upsertLlmProvider(updated)
      await loadLlmProviders()
    } catch (err) {
      alert(`${t('common.updateFailed')}: ${err instanceof Error ? err.message : t('common.unknownError')}`)
    }
  }

  const handleFormSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    try {
      const provider: LlmProvider = {
        ...formData,
        id: editingProvider?.id || '',
        hasApiKey: !!(formData.apiKey || editingProvider?.hasApiKey)
      }
      await upsertLlmProvider(provider)
      await loadLlmProviders()
      resetForm()
    } catch (err) {
      alert(`${t('common.saveFailed')}: ${err instanceof Error ? err.message : t('common.unknownError')}`)
    }
  }

  const resetForm = () => {
    setFormData({ name: '', provider: 'openai', baseUrl: '', model: '', apiKey: '', isActive: false })
    setEditingProvider(null)
    setIsAddingProvider(false)
  }

  const openEditForm = (provider: LlmProvider) => {
    setFormData({
      name: provider.name,
      provider: provider.provider,
      baseUrl: provider.baseUrl,
      model: provider.model,
      apiKey: '',
      isActive: provider.isActive ?? false
    })
    setEditingProvider(provider)
    setIsAddingProvider(true)
  }

  useEffect(() => {
    void refresh()
    void loadLlmProviders()
  }, [])

  const handleCopyCommand = async () => {
    if (!versionData) return
    try {
      await navigator.clipboard.writeText(versionData.update_command)
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    } catch (err) {
      console.error('Failed to copy command:', err)
    }
  }

  return (
    <div className="min-h-full bg-slate-50 px-6 py-6 text-slate-950">
      <div className="mx-auto max-w-5xl space-y-5">
        <header className="flex items-start justify-between gap-4">
          <div>
            <p className="text-[13px] font-medium text-blue-600">{t('settings.eyebrow')}</p>
            <h2 className="mt-1 text-[26px] font-semibold tracking-normal">{t('settings.title')}</h2>
            <p className="mt-2 max-w-2xl text-[14px] leading-6 text-slate-500">
              {t('settings.subtitle')}
            </p>
          </div>
          <button
            onClick={() => void refresh()}
            className="flex h-9 items-center gap-2 rounded-md border border-slate-200 bg-white px-3 text-[13px] font-medium text-slate-600 shadow-sm hover:bg-slate-50"
          >
            <RefreshCw size={15} />
            {t('common.refresh')}
          </button>
        </header>

        {error && (
          <div className="rounded-lg border border-red-100 bg-red-50 p-3 text-xs text-red-800" role="alert">
            {error}
          </div>
        )}

        {loading ? (
          <div className="rounded-lg border border-slate-200 bg-white px-5 py-12 text-center text-[14px] text-slate-400 shadow-sm">
            {t('common.loading')}
          </div>
        ) : (
          <div className="grid gap-5 lg:grid-cols-[1fr_1fr]">
            <SettingCard title={t('settings.language')} description={t('settings.languageDesc')}>
              <div className="grid grid-cols-3 gap-2">
                {(['en', 'zh-TW', 'ko'] as const).map((value) => (
                  <button
                    key={value}
                    onClick={() => setLocale(value)}
                    className={`h-10 rounded-md border text-[13px] font-medium ${
                      locale === value
                        ? 'border-blue-200 bg-blue-50 text-blue-600'
                        : 'border-slate-200 text-slate-600 hover:bg-slate-50'
                    }`}
                  >
                    {t(`settings.language.${value}`)}
                  </button>
                ))}
              </div>
            </SettingCard>

            <SettingCard title={t('settings.ragProfile')} description={t('settings.ragProfileDesc')}>
              <div className="grid grid-cols-3 gap-2">
                {(['fast', 'balanced', 'precise'] as RAGProfile[]).map((value) => (
                  <button
                    key={value}
                    onClick={() => setProfile(value)}
                    className={`h-10 rounded-md border text-[13px] font-medium capitalize ${
                      profile === value
                        ? 'border-blue-200 bg-blue-50 text-blue-600'
                        : 'border-slate-200 text-slate-600 hover:bg-slate-50'
                    }`}
                  >
                    {t(`settings.profile.${value}`)}
                  </button>
                ))}
              </div>
              <p className="mt-3 text-[12px] leading-5 text-slate-500">
                {profile === 'fast'
                  ? t('settings.profile.fastDesc')
                  : profile === 'balanced'
                    ? t('settings.profile.balancedDesc')
                    : t('settings.profile.preciseDesc')}
              </p>
            </SettingCard>

            <SettingCard title={t('settings.server')} description={t('settings.serverDesc')}>
              <div className="grid gap-3 sm:grid-cols-2">
                <Field label={t('common.status')} value={health?.status || t('common.unknown')} />
                <Field label={t('settings.version')} value={health?.version || t('common.unknown')} />
                <Field label={t('settings.workspace')} value={workbench?.workspace.name || t('common.unknown')} />
                <Field label={t('settings.mode')} value={workbench?.workspace.mode || t('common.unknown')} />
              </div>
              {checkingVersion && (
                <div className="mt-3 flex items-center gap-1.5 text-xs text-slate-400">
                  <RefreshCw className="h-3 w-3 animate-spin text-blue-500" />
                  <span>正在向 GitHub 檢測最新核心版本...</span>
                </div>
              )}
              {versionData?.has_update && !checkingVersion && (
                <div className="mt-4 border-t border-slate-100 pt-4">
                  <div className="rounded-lg border border-amber-200 bg-amber-50/50 p-4">
                    <div className="flex items-start gap-2.5">
                      <AlertCircle className="mt-0.5 h-4 w-4 shrink-0 text-amber-600" />
                      <div className="flex-1">
                        <h4 className="text-[13px] font-semibold text-amber-900">發現新版本可用：v{versionData.latest_version}</h4>
                        <p className="mt-1 text-xs text-amber-700 leading-relaxed">
                          您的目前版本為 v{versionData.current_version}。請複製下方指令，在您的本機終端機中執行即可快速升級：
                        </p>
                        <div className="mt-3 flex items-center gap-1.5 rounded border border-slate-800 bg-slate-900 px-3 py-1.5 font-mono text-[11px] text-slate-200">
                          <span className="flex-1 truncate select-all">{versionData.update_command}</span>
                          <button
                            onClick={() => void handleCopyCommand()}
                            className="ml-2 inline-flex h-6 w-6 items-center justify-center rounded bg-slate-800 text-slate-300 hover:bg-slate-700 hover:text-white transition-colors"
                            title="複製升級指令"
                          >
                            {copied ? <Check className="h-3.5 w-3.5 text-emerald-500" /> : <Copy className="h-3.5 w-3.5" />}
                          </button>
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
              )}
            </SettingCard>

            <SettingCard title={t('settings.corpusReadiness')} description={t('settings.corpusReadinessDesc')}>
              <div className="grid gap-3 sm:grid-cols-2">
                <Field label={t('common.documents')} value={workbench?.corpus.documents ?? 0} />
                <Field label={t('common.chunks')} value={workbench?.corpus.chunks ?? 0} />
                <Field label={t('dashboard.connectors')} value={`${workbench?.connectors.active ?? 0}/${workbench?.connectors.total ?? 0} ${t('common.active')}`} />
                <Field label={t('settings.modelStatus')} value={workbench?.health.modelStatus || t('common.unknown')} />
              </div>
            </SettingCard>

            <SettingCard className="lg:col-span-2" title="BYOK 自備金鑰 LLM 設定" description="管理您自行設定的 LLM Provider。一次只能啟用一個 Provider，啟用後其他 Provider 將自動失效。">
              {llmLoading ? (
                <div className="flex items-center justify-center py-8">
                  <Loader2 className="h-6 w-6 animate-spin text-blue-600" />
                  <span className="ml-2 text-sm text-slate-500">正在加載 LLM Provider...</span>
                </div>
              ) : llmError ? (
                <div className="rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
                  {llmError}
                </div>
              ) : (
                <div className="space-y-4">
                  {llmProviders.length === 0 ? (
                    <div className="text-center py-8 text-slate-400">
                      <Key className="h-12 w-12 mx-auto mb-3 text-slate-300" />
                      <p className="text-sm">尚無已配置的 LLM Provider</p>
                      <p className="text-xs mt-1">點擊下方按鈕新增您的第一個 Provider</p>
                    </div>
                  ) : (
                    <div className="space-y-3">
                      {llmProviders.map((provider) => (
                        <div key={provider.id} className="rounded-lg border border-slate-200 bg-slate-50 p-4 hover:bg-slate-100 transition-colors">
                          <div className="flex items-start justify-between mb-3">
                            <div className="flex-1">
                              <div className="flex items-center gap-2 mb-1">
                                <h4 className="font-medium text-slate-900 text-sm">{provider.name}</h4>
                                {provider.isActive && (
                                  <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-green-100 text-green-700 text-xs font-medium">
                                    <Power size={12} className="text-green-600" />
                                    啟用中
                                  </span>
                                )}
                              </div>
                              <div className="text-xs text-slate-600 space-y-1">
                                <p><span className="inline-block w-20">API 種類:</span> {provider.provider}</p>
                                <p><span className="inline-block w-20">伺服器網址:</span> 
                                  <span className="font-mono text-[11px]">{provider.baseUrl}</span>
                                </p>
                                <p><span className="inline-block w-20">模型名稱:</span> {provider.model}</p>
                                <p><span className="inline-block w-20">建立時間:</span> {provider.createdAt ? new Date(provider.createdAt).toLocaleString() : '-'}</p>
                              </div>
                            </div>
                            <div className="flex gap-2 ml-4">
                              <button
                                onClick={() => handleToggleActive(provider)}
                                className={`p-2 rounded-md transition-colors ${
                                  provider.isActive
                                    ? 'bg-green-100 text-green-600 hover:bg-green-200'
                                    : 'bg-slate-200 text-slate-600 hover:bg-slate-300'
                                }`}
                                title={provider.isActive ? '已啟用，點擊停用' : '未啟用，點擊啟用'}
                              >
                                {provider.isActive ? <Power size={16} /> : <ToggleLeft size={16} />}
                              </button>
                              <button
                                onClick={() => openEditForm(provider)}
                                className="p-2 rounded-md bg-blue-100 text-blue-600 hover:bg-blue-200 transition-colors"
                                title="編輯 Provider"
                              >
                                <Plus size={16} />
                              </button>
                              <button
                                onClick={() => handleTestProvider(provider)}
                                disabled={testLoadingId === provider.id}
                                className="p-2 rounded-md bg-slate-100 text-slate-600 hover:bg-slate-200 transition-colors disabled:opacity-50"
                                title="測試連線"
                              >
                                {testLoadingId === provider.id ? (
                                  <Loader2 className="h-4 w-4 animate-spin" />
                                ) : (
                                  <Play size={16} />
                                )}
                              </button>
                              <button
                                onClick={() => handleDeleteProvider(provider.id || '')}
                                className={`p-2 rounded-md transition-colors ${
                                  deleteConfirmId === provider.id
                                    ? 'bg-red-200 text-red-700'
                                    : 'bg-slate-100 text-slate-600 hover:bg-slate-200'
                                }`}
                                title={deleteConfirmId === provider.id ? '再次點擊確認刪除' : '刪除 Provider'}
                              >
                                {deleteConfirmId === provider.id ? (
                                  <X size={16} />
                                ) : (
                                  <Trash2 size={16} />
                                )}
                              </button>
                            </div>
                          </div>
                        </div>
                      ))}
                    </div>
                  )}

                  {isAddingProvider && (
                    <div className="mt-4 rounded-lg border-2 border-dashed border-slate-300 bg-slate-50 p-6">
                      <h4 className="text-sm font-medium text-slate-900 mb-4">
                        {editingProvider ? '編輯 LLM Provider' : '新增 LLM Provider'}
                      </h4>
                      <form onSubmit={handleFormSubmit} className="space-y-4">
                        <div className="grid gap-4 sm:grid-cols-2">
                          <div>
                            <label className="block text-xs font-medium text-slate-700 mb-2">名稱</label>
                            <input
                              type="text"
                              required
                              value={formData.name}
                              onChange={(e) => setFormData({...formData, name: e.target.value})}
                              className="w-full px-3 py-2 text-sm border border-slate-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
                              placeholder="例如：LiteLLM 測試"
                            />
                          </div>
                          <div>
                            <label className="block text-xs font-medium text-slate-700 mb-2">API 種類</label>
                            <select
                              value={formData.provider}
                              onChange={(e) => setFormData({...formData, provider: e.target.value})}
                              className="w-full px-3 py-2 text-sm border border-slate-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
                            >
                              <option value="openai">OpenAI</option>
                              <option value="litellm">LiteLLM</option>
                              <option value="anthropic">Anthropic</option>
                              <option value="azure">Azure OpenAI</option>
                              <option value="google">Google</option>
                            </select>
                          </div>
                          <div className="sm:col-span-2">
                            <label className="block text-xs font-medium text-slate-700 mb-2">API 網址 (Base URL)</label>
                            <input
                              type="url"
                              required
                              value={formData.baseUrl}
                              onChange={(e) => setFormData({...formData, baseUrl: e.target.value})}
                              className="w-full px-3 py-2 text-sm border border-slate-200 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 font-mono"
                              placeholder="例如：https://api.openai.com/v1"
                            />
                          </div>
                          <div className="sm:col-span-2">
                            <label className="block text-xs font-medium text-slate-700 mb-2">模型名稱</label>
                            <input
                              type="text"
                              required
                              value={formData.model}
                              onChange={(e) => setFormData({...formData, model: e.target.value})}
                              className="w-full px-3 py-2 text-sm border border-slate-200 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
                              placeholder="例如：gpt-4o 或 claude-3-5-sonnet"
                            />
                          </div>
                          <div className="sm:col-span-2">
                            <label className="block text-xs font-medium text-slate-700 mb-2">API 金鑰 (若不修改留空)</label>
                            <input
                              type="password"
                              value={formData.apiKey}
                              onChange={(e) => setFormData({...formData, apiKey: e.target.value})}
                              className="w-full px-3 py-2 text-sm border border-slate-200 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
                              placeholder="留空表示使用現有設定"
                            />
                          </div>
                          <div>
                            <label className="flex items-center gap-2">
                              <input
                                type="checkbox"
                                checked={formData.isActive}
                                onChange={(e) => setFormData({...formData, isActive: e.target.checked})}
                                className="w-4 h-4 text-blue-600 border-slate-300 rounded focus:ring-blue-500"
                              />
                              <span className="text-xs font-medium text-slate-700">立即啟用</span>
                            </label>
                          </div>
                        </div>
                        <div className="flex gap-3 pt-4 border-t border-slate-200">
                          <button
                            type="submit"
                            className="px-4 py-2 bg-blue-600 text-white text-sm font-medium rounded-md hover:bg-blue-700 transition-colors"
                          >
                            {editingProvider ? '保存變更' : '新增 Provider'}
                          </button>
                          <button
                            type="button"
                            onClick={resetForm}
                            className="px-4 py-2 bg-slate-200 text-slate-700 text-sm font-medium rounded-md hover:bg-slate-300 transition-colors"
                          >
                            取消
                          </button>
                          {editingProvider && (
                            <button
                              type="button"
                              onClick={() => handleTestProvider(editingProvider)}
                              disabled={testLoadingId === editingProvider.id}
                              className="px-4 py-2 bg-green-600 text-white text-sm font-medium rounded-md hover:bg-green-700 transition-colors disabled:opacity-50 ml-auto"
                            >
                              {testLoadingId === editingProvider.id ? (
                                <>
                                  <Loader2 className="inline h-4 w-4 animate-spin mr-2" />
                                  測試中...
                                </>
                              ) : (
                                '測試此設定'
                              )}
                            </button>
                          )}
                        </div>
                      </form>
                    </div>
                  )}

                  {!isAddingProvider && (
                    <button
                      onClick={() => setIsAddingProvider(true)}
                      className="w-full mt-4 py-3 border-2 border-dashed border-slate-300 text-slate-600 text-sm font-medium rounded-md hover:bg-slate-50 transition-colors flex items-center justify-center gap-2"
                    >
                      <Plus size={16} />
                      新增 LLM Provider
                    </button>
                  )}
                </div>
              )}
            </SettingCard>

          </div>
        )}
      </div>
    </div>
  )
}
