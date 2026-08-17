import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react'
import { SourceCard } from './SourceCard'
import { Markdown } from '../ui/Markdown'
import type { ChatMessage as ChatMessageType, SearchResult } from '../../lib/types'
import { Info, X } from 'lucide-react'
import { useAppStore } from '../../stores/appStore'
import { translate as tr } from '../../lib/i18n'

interface Props {
  message: ChatMessageType
  isStreaming?: boolean
  onFeedback?: (type: 'positive' | 'negative') => void
}

export function ChatMessage({ message, isStreaming, onFeedback }: Props) {
  const { locale } = useAppStore()
  const t = (key: string, values?: Record<string, string | number>) => tr(locale, key, values)
  const [selectedSource, setSelectedSource] = useState<SearchResult | null>(null)
  // 1.1.4: citation 點擊後高亮的來源卡 index（幾秒後自動清除）
  const [highlightedSource, setHighlightedSource] = useState<number | null>(null)
  const rawSources = useMemo(() => message.sources || [], [message.sources])

  // 去重：同一文件僅顯示一張卡（LLM 依 chunk 編號，但前端以文件為單位呈現）。
  // 不先 slice，讓 [N] 都能對應到原始 raw 清單。
  const dedupedSources = useMemo<SearchResult[]>(() => {
    const result: SearchResult[] = []
    const seenDocs = new Set<string>()
    for (const src of rawSources) {
      const docKey = src.documentId || src.sourcePath || src.content
      if (!seenDocs.has(docKey)) {
        seenDocs.add(docKey)
        result.push(src)
      }
    }
    return result
  }, [rawSources])

  // raw source index -> 去重後卡片 index。
  // citation [N] 對應 rawSources[N-1]，即使該文件有數個 chunk 被合併到同一張卡，
  // 也會正確落在「第一張同文件卡」。
  const rawToCardIndex = useMemo(() => {
    const map: Record<number, number> = {}
    const docToCard = new Map<string, number>()
    let cardIdx = 0
    rawSources.forEach((src, rawIdx) => {
      const docKey = src.documentId || src.sourcePath || src.content
      if (!docToCard.has(docKey)) {
        docToCard.set(docKey, cardIdx)
        cardIdx++
      }
      map[rawIdx] = docToCard.get(docKey)!
    })
    return map
  }, [rawSources])

  // Citation 點擊：平滑滾動到「本則訊息」的對應來源卡並套用高亮外框（3 秒後自動清除）
  const handleCitationClick = useCallback(
    (num: number) => {
      const cardIndex = rawToCardIndex[num - 1]
      if (cardIndex === undefined) return
      setHighlightedSource(cardIndex)
      document
        .getElementById(`source-card-${message.id}-${cardIndex}`)
        ?.scrollIntoView({ behavior: 'smooth', block: 'center' })
      window.setTimeout(() => setHighlightedSource((cur) => (cur === cardIndex ? null : cur)), 3000)
    },
    [rawToCardIndex, message.id],
  )

  useEffect(() => {
    if (!selectedSource) return
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setSelectedSource(null)
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [selectedSource])
  const isUser = message.role === 'user'

  const confidence = message.confidence?.score
  const bestSourceScore = dedupedSources.length > 0 ? Math.max(...dedupedSources.map((source) => source.score)) : undefined
  const metricScore = bestSourceScore ?? confidence
  const metricLabel = bestSourceScore !== undefined ? t('chat.evidenceMatch') : t('chat.confidence')
  const sourceHeading = selectedSource?.headingHierarchy?.join(' / ')

  const renderCitation = useCallback((num: number): ReactNode => {
    const cardIndex = rawToCardIndex[num - 1]
    const source = cardIndex !== undefined ? dedupedSources[cardIndex] : undefined
    if (source) {
      const filename = source.sourcePath.split('/').pop() || source.sourcePath
      const tooltipText = `${filename}\n\n${source.content.slice(0, 100)}${source.content.length > 100 ? '...' : ''}`
      return (
        <button
          type="button"
          className="mx-0.5 inline-flex items-center justify-center rounded-md border border-blue-100 bg-blue-50 px-1.5 py-0.5 align-baseline text-[11px] font-bold leading-none text-blue-600 transition-colors hover:bg-blue-100 focus-visible:ring-2 focus-visible:ring-blue-200"
          onClick={() => handleCitationClick(num)}
          title={tooltipText}
          aria-label={`${tr(locale, 'chat.citationPreview')} ${num} - ${source.sourcePath}`}
        >
          [{num}]
        </button>
      )
    }

    return (
      <span
        className="mx-0.5 inline-block rounded-md border border-orange-300 bg-orange-50 px-1.5 py-0.5 align-baseline text-[11px] font-bold leading-none text-orange-600"
        title={`${tr(locale, 'chat.citationOutOfBounds')} ${num}`}
      >
        [{num}]
      </span>
    )
  }, [dedupedSources, rawToCardIndex, handleCitationClick, locale])

  return (
    <div className={`flex w-full ${isUser ? 'justify-end' : 'justify-start'}`}>
      <div className={`${isUser ? 'max-w-[78%]' : 'min-w-0 max-w-full flex-1'}`}>
        {isUser ? (
          <div className="rounded-lg bg-blue-600 px-4 py-3 text-[15px] leading-relaxed text-white">
            <p className="break-words [overflow-wrap:anywhere]">{message.content}</p>
          </div>
        ) : (
          <article className="min-w-0 max-w-full rounded-lg border border-slate-200 bg-white px-6 py-6 shadow-sm">
            <div className="flex items-start justify-between gap-6">
              <div className="min-w-0 flex-1">
                <p className="mb-2.5 text-[13px] font-medium text-blue-600">{t('chat.answer')}</p>
                <div className="prose prose-sm max-w-none prose-slate text-[15px] leading-6 [&_p:first-child]:mt-0 [&_p:first-child]:font-semibold [&_p:first-child]:text-slate-950 [&_p]:my-2 [overflow-wrap:anywhere]">
                  <Markdown content={message.content} citationRenderer={renderCitation} />
                </div>
              </div>
              {metricScore !== undefined && (
                <div className="w-[128px] shrink-0 pt-7">
                  <div className="mb-2 flex items-center justify-between gap-2">
                    <span className="flex items-center gap-1 text-[11px] font-medium text-slate-500">
                      {metricLabel}
                      <Info size={11} strokeWidth={1.8} className="text-slate-300" />
                    </span>
                    <span className="text-[15px] font-semibold text-emerald-500">{Math.round(metricScore * 100)}%</span>
                  </div>
                  <div className="h-1 rounded-full bg-slate-200">
                    <div className="h-1 rounded-full bg-emerald-500" style={{ width: `${Math.min(100, Math.round(metricScore * 100))}%` }} />
                  </div>
                </div>
              )}
            </div>

            {dedupedSources.length > 0 && (
              <div className="mt-6 min-w-0 max-w-full border-t border-slate-200 pt-5">
                <div className="mb-4 flex items-center justify-between gap-4">
                  <div className="flex items-center gap-2">
                    <span className="text-[13px] font-semibold text-slate-900">{t('chat.sources')}</span>
                    <span className="rounded-md bg-blue-50 px-1.5 py-0.5 text-[11px] font-semibold text-blue-600">
                      {dedupedSources.length}
                    </span>
                  </div>
                </div>
                <div className="grid w-full min-w-0 max-w-full grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
                  {dedupedSources.map((source, i) => (
                    <div key={`${source.chunkId}-${i}`} id={`source-card-${message.id}-${i}`} className="min-w-0 max-w-full overflow-hidden scroll-mt-6">
                      <SourceCard
                        source={source}
                        onOpen={setSelectedSource}
                        openLabel={t('chat.openSource')}
                        highlighted={highlightedSource === i}
                      />
                    </div>
                  ))}
                </div>
              </div>
            )}

            {!isStreaming && onFeedback && (
              <div className="mt-4 flex gap-2">
                <button
                  onClick={() => onFeedback?.('positive')}
                  className="rounded-md border border-slate-200 px-3 py-1.5 text-xs font-medium text-slate-500 transition-colors hover:border-emerald-200 hover:bg-emerald-50 hover:text-emerald-700"
                >
                  {t('chat.helpful')}
                </button>
                <button
                  onClick={() => onFeedback?.('negative')}
                  className="rounded-md border border-slate-200 px-3 py-1.5 text-xs font-medium text-slate-500 transition-colors hover:border-red-200 hover:bg-red-50 hover:text-red-700"
                >
                  {t('chat.notUseful')}
                </button>
              </div>
            )}
          </article>
        )}

        {selectedSource && (
          <div role="dialog" aria-modal="true" aria-label={t('chat.sourcePreview')} className="fixed inset-0 z-50 pt-24">
            <div className="absolute inset-0 bg-slate-950/50" onClick={() => setSelectedSource(null)} />
            <div className="relative mx-auto max-w-3xl max-h-[86vh] overflow-auto" onClick={(e) => e.stopPropagation()}>
              <div className="flex items-start justify-between gap-4 px-5 py-4">
                <div className="min-w-0">
                  <p className="text-[12px] font-semibold text-blue-600">{t('chat.sourcePreview')}</p>
                  <h3 className="mt-1 truncate text-[17px] font-semibold text-slate-950">
                    {selectedSource.sourcePath.split(/[/\\]/).pop() || selectedSource.sourcePath}
                  </h3>
                  <p className="mt-1 break-words [overflow-wrap:anywhere] text-[12px] text-slate-500">{selectedSource.sourcePath}</p>
                </div>
                <button
                  type="button"
                  onClick={() => setSelectedSource(null)}
                  className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-slate-400 hover:bg-slate-100 hover:text-slate-700"
                  aria-label={t('common.close')}
                >
                  <X size={17} />
                </button>
              </div>
              <div className="mx-5 mb-5 overflow-hidden rounded-lg border border-slate-200 bg-white shadow-xl">
                <div className="px-5 py-4">
                <div className="mb-4 grid gap-3 sm:grid-cols-2">
                  <div className="rounded-md border border-slate-200 px-3 py-2">
                    <p className="text-[11px] font-semibold uppercase text-slate-400">{t('chat.match')}</p>
                    <p className="mt-1 text-[14px] font-semibold text-slate-900">{Math.round(selectedSource.score * 100)}%</p>
                  </div>
                  <div className="rounded-md border border-slate-200 px-3 py-2">
                    <p className="text-[11px] font-semibold uppercase text-slate-400">{t('chat.sourcePath')}</p>
                    <p className="mt-1 truncate text-[13px] text-slate-700">{sourceHeading || selectedSource.sourceType}</p>
                  </div>
                </div>
                <p className="mb-2 text-[12px] font-semibold text-slate-500">{t('chat.chunkContent')}</p>
                {selectedSource.sourcePath.endsWith('.md') ? (
                  <div className="prose prose-sm max-w-none min-w-0 text-[13px] leading-6 text-slate-800 [overflow-wrap:anywhere] dark:text-slate-200">
                    <Markdown content={selectedSource.content} />
                  </div>
                ) : (
                  <pre className="overflow-x-auto rounded-md bg-slate-50 p-4 text-[13px] leading-6 text-slate-800">
                    <code>{selectedSource.content}</code>
                  </pre>
                )}
              </div>
              </div>
            </div>
          </div>
        )}
        {isStreaming && !isUser && (
          <span className="ml-1 mt-2 inline-block h-4 w-1 animate-pulse bg-blue-500" />
        )}
      </div>
    </div>
  )
}
