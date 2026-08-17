import type { SearchResult } from '../../lib/types'
import { ExternalLink, FileText } from 'lucide-react'

interface Props {
  source: SearchResult
  onOpen: (source: SearchResult) => void
  openLabel: string
  /** 由 citation 連結觸發的高亮狀態（smooth-scroll 後套用外框） */
  highlighted?: boolean
}

export function SourceCard({ source, onOpen, openLabel, highlighted }: Props) {
  const filename = source.sourcePath.split(/[/\\]/).pop() || source.sourcePath
  const sourceHost = source.sourcePath.includes('://')
    ? source.sourcePath.split('://')[1]?.split('/')[0]
    : source.sourceType || 'indexed source'
  const score = Math.max(0, Math.min(100, Math.round(source.score * 100)))

  return (
    <button
      type="button"
      onClick={() => onOpen(source)}
      className={`flex w-full min-w-0 max-w-full items-center gap-2.5 overflow-hidden rounded-md border p-2 text-left transition-colors focus:outline-none focus:ring-2 focus:ring-blue-200 ${
        highlighted
          ? 'border-blue-300 bg-blue-50 ring-2 ring-blue-200'
          : 'border-transparent hover:border-blue-100 hover:bg-blue-50'
      }`}
      title={`${openLabel}: ${source.sourcePath}`}
    >
      <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-white text-blue-600 focus-visible:ring-2 focus-visible:ring-blue-200">
        <FileText size={17} strokeWidth={2} />
      </div>
      <div className="min-w-0 flex-1 overflow-hidden">
        <p className="block w-full min-w-0 truncate text-[12px] font-semibold text-slate-900" title={filename}>{filename}</p>
        <p className="block w-full min-w-0 truncate text-[11px] text-slate-500" title={sourceHost}>{sourceHost}</p>
      </div>
      <span className="flex h-5 min-w-[36px] shrink-0 items-center justify-center rounded bg-slate-100 px-1.5 text-[10px] font-semibold text-slate-500">
        {score}%
      </span>
      <ExternalLink size={13} className="shrink-0 text-slate-300" />
    </button>
  )
}
