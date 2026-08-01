import { useEffect, useState } from 'react'
import type { ReactNode } from 'react'
import { CheckCircle2, Monitor, Moon, RefreshCw, Server, Sun, AlertCircle, Copy, Check } from 'lucide-react'
import { getHealth, getModelBenchmarks, getWorkbench, checkVersion } from '../../lib/api'
import type { VersionCheckResponse } from '../../lib/api'
import { useAppStore } from '../../stores/appStore'
import type { RAGProfile, WorkbenchResponse } from '../../lib/types'
import { translate as tr } from '../../lib/i18n'

interface BenchmarkModel {
  name: string
  version: string
  capabilities: Record<string, boolean | undefined>
  health: { healthy: boolean; message?: string } | null
  generation: { latencyMs: number; tokensPerSec: number } | { error: string } | null
  embedding: { latencyMs: number; textsPerSec: number } | { error: string } | null
}

function SettingCard({ title, description, children }: { title: string; description?: string; children: ReactNode }) {
  return (
    <section className="rounded-lg border border-slate-200 bg-white p-5 shadow-sm">
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
  const { profile, setProfile, theme, setTheme, locale, setLocale } = useAppStore()
  const t = (key: string, values?: Record<string, string | number>) => tr(locale, key, values)
  const [health, setHealth] = useState<{ status: string; version: string } | null>(null)
  const [workbench, setWorkbench] = useState<WorkbenchResponse | null>(null)
  const [models, setModels] = useState<BenchmarkModel[]>([])
  const [loading, setLoading] = useState(true)
  const [benchmarksLoading, setBenchmarksLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  
  // 版本更新相關狀態
  const [versionData, setVersionData] = useState<VersionCheckResponse | null>(null)
  const [checkingVersion, setCheckingVersion] = useState(false)
  const [copied, setCopied] = useState(false)

  const refresh = async () => {
    setLoading(true)
    setBenchmarksLoading(true)
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

    // 2. 異步在背景非阻塞執行耗時的大模型效能跑分（5~10秒，不卡死主頁面）
    try {
      const modelData = await getModelBenchmarks().catch(() => ({ benchmarks: [] as BenchmarkModel[] }))
      setModels(modelData.benchmarks ?? [])
    } catch (err) {
      console.warn('Failed to load benchmarks:', err)
    } finally {
      setBenchmarksLoading(false)
    }
  }

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

  useEffect(() => {
    void refresh()
  }, [])

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
          <div className="rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">{error}</div>
        )}

        {loading ? (
          <div className="rounded-lg border border-slate-200 bg-white px-5 py-12 text-center text-[14px] text-slate-400 shadow-sm">
            {t('common.loading')}
          </div>
        ) : (
          <div className="grid gap-5 lg:grid-cols-[1fr_1fr]">
            <SettingCard title={t('settings.appearance')} description={t('settings.appearanceDesc')}>
              <div className="grid grid-cols-3 gap-2">
                {([
                  ['system', Monitor],
                  ['light', Sun],
                  ['dark', Moon],
                ] as const).map(([value, Icon]) => (
                  <button
                    key={value}
                    onClick={() => setTheme(value)}
                    className={`flex h-10 items-center justify-center gap-2 rounded-md border text-[13px] font-medium capitalize ${
                      theme === value
                        ? 'border-blue-200 bg-blue-50 text-blue-600'
                        : 'border-slate-200 text-slate-600 hover:bg-slate-50'
                    }`}
                  >
                    <Icon size={15} />
                    {t(`settings.theme.${value}`)}
                  </button>
                ))}
              </div>
            </SettingCard>

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
              {versionData && !checkingVersion && (
                <div className="mt-4 border-t border-slate-100 pt-4">
                  {versionData.has_update ? (
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
                              {copied ? <Check className="h-3.5 w-3 3.5 text-emerald-500" /> : <Copy className="h-3.5 w-3.5" />}
                            </button>
                          </div>
                        </div>
                      </div>
                    </div>
                  ) : (
                    <div className="flex items-center gap-2 text-xs text-slate-500">
                      <CheckCircle2 className="h-4 w-4 text-emerald-500" />
                      <span>已是最新版本 (v{versionData.current_version})</span>
                    </div>
                  )}
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

            <section className="rounded-lg border border-slate-200 bg-white p-5 shadow-sm lg:col-span-2">
              <div className="flex items-center gap-2">
                <Server size={17} className="text-slate-500" />
                <h3 className="text-[15px] font-semibold text-slate-950">{t('settings.modelProviders')}</h3>
              </div>
              {benchmarksLoading ? (
                <div className="mt-4 flex items-center justify-center gap-2.5 rounded-lg border border-slate-100 bg-slate-50/50 p-6 text-[13px] text-slate-500">
                  <RefreshCw className="h-4 w-4 animate-spin text-blue-600" />
                  <span>正在對大模型進行本地硬體跑分基準測試中，請稍候...</span>
                </div>
              ) : models.length === 0 ? (
                <p className="mt-4 text-[13px] text-slate-400">{t('settings.noModels')}</p>
              ) : (
                <div className="mt-4 divide-y divide-slate-100 rounded-lg border border-slate-200">
                  {models.map((model) => (
                    <div key={model.name} className="grid gap-3 px-4 py-3 md:grid-cols-[1fr_150px_150px]">
                      <div className="min-w-0">
                        <div className="flex items-center gap-2">
                          <p className="truncate text-[14px] font-semibold text-slate-900">{model.name}</p>
                          <CheckCircle2 size={14} className={model.health?.healthy ? 'text-emerald-500' : 'text-slate-300'} />
                        </div>
                        <p className="mt-1 text-[12px] text-slate-400">v{model.version} · {model.health?.message || t('common.notRecorded')}</p>
                      </div>
                      <p className="text-[12px] text-slate-500">
                        {t('settings.generation')}<br />
                        <span className="font-medium text-slate-800">
                          {model.generation && 'latencyMs' in model.generation ? `${model.generation.latencyMs}ms` : '-'}
                        </span>
                      </p>
                      <p className="text-[12px] text-slate-500">
                        {t('settings.embedding')}<br />
                        <span className="font-medium text-slate-800">
                          {model.embedding && 'latencyMs' in model.embedding ? `${model.embedding.latencyMs}ms` : '-'}
                        </span>
                      </p>
                    </div>
                  ))}
                </div>
              )}
            </section>
          </div>
        )}
      </div>
    </div>
  )
}
