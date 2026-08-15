import ReactMarkdown from 'react-markdown'
import rehypeHighlight from 'rehype-highlight'
import { memo, useState, useCallback, type ReactNode } from 'react'
import { Check, Copy } from 'lucide-react'
import type { Components } from 'react-markdown'
import { translate as tr } from '../../lib/i18n'
import { useAppStore } from '../../stores/appStore'

interface MarkdownProps {
  content: string
  className?: string
  /** 自訂節點渲染器（例如 citation 互動標籤） */
  components?: Components
}

/** 從 react-markdown 的 children 遞迴萃取純文字（code 內容為字串陣列） */
function extractText(node: ReactNode): string {
  if (node == null) return ''
  if (typeof node === 'string' || typeof node === 'number') return String(node)
  if (Array.isArray(node)) return node.map(extractText).join('')
  if (typeof node === 'object' && 'props' in node) {
    return extractText((node as { props: { children?: ReactNode } }).props.children)
  }
  return ''
}

/** 程式碼區塊：語法高亮 + 右上角浮動 Copy 按鈕 */
function CodeBlock({ children }: { children: ReactNode }) {
  const [copied, setCopied] = useState(false)
  const locale = useAppStore((s) => s.locale)
  const code = extractText(children)

  const handleCopy = useCallback(() => {
    void navigator.clipboard.writeText(code).then(() => {
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    })
  }, [code])

  return (
    <div className="group relative">
      <pre className="overflow-x-auto rounded-md bg-slate-950 p-3 text-[13px] leading-6 text-slate-100">
        {children}
      </pre>
      <button
        type="button"
        onClick={handleCopy}
        aria-label={tr(locale, 'chat.copyCode')}
        title={tr(locale, 'chat.copyCode')}
        className="absolute right-2 top-2 flex h-7 w-7 items-center justify-center rounded-md bg-slate-800/80 text-slate-300 opacity-0 transition-opacity hover:bg-slate-700 hover:text-white focus:opacity-100 group-hover:opacity-100"
      >
        {copied ? <Check size={14} className="text-green-400" /> : <Copy size={14} />}
      </button>
    </div>
  )
}

/**
 * 共用 Markdown 渲染元件。
 * - react-markdown 9 + rehype-highlight（程式碼高亮）
 * - 自訂 components 統一 code/pre/table/blockquote/a 樣式，與設計語言一致
 * - 預設不渲染 raw HTML（react-markdown 安全預設）
 * - memo：串流時僅在 content 實際變化才重解析，避免每 token 全量重掛載（§3708）
 */
export const Markdown = memo(function Markdown({ content, className, components }: MarkdownProps) {
  return (
    <div className={`opendoc-markdown ${className ?? ''}`.trim()}>
      <ReactMarkdown
        rehypePlugins={[rehypeHighlight]}
        components={{
          // code：一律輸出純 code（有 language- class 由 hljs 高亮）；
          // 行內 vs 區塊由 CSS `.opendoc-markdown :not(pre) > code` 依 DOM 結構決定，
          // 避免無語言標記的 fenced block 被誤判為行內 code（黑底白框 bug）
          code({ className: cls, children, node: _node, ...props }) {
            return (
              <code className={cls} {...props}>
                {children}
              </code>
            )
          },
          pre({ children }) {
            return <CodeBlock>{children}</CodeBlock>
          },
          table({ children }) {
            return (
              <div className="overflow-x-auto">
                <table className="w-full border-collapse text-[13px]">{children}</table>
              </div>
            )
          },
          th({ children }) {
            return (
              <th className="border border-slate-200 bg-slate-50 px-3 py-2 text-left font-semibold text-slate-700 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-200">
                {children}
              </th>
            )
          },
          td({ children }) {
            return (
              <td className="border border-slate-200 px-3 py-2 text-slate-700 dark:border-gray-700 dark:text-gray-300">
                {children}
              </td>
            )
          },
          blockquote({ children }) {
            return (
              <blockquote className="border-l-4 border-blue-200 bg-blue-50/50 px-4 py-2 text-slate-600 dark:border-blue-800 dark:bg-gray-800/50 dark:text-gray-300">
                {children}
              </blockquote>
            )
          },
          a({ href, children }) {
            return (
              <a
                href={href}
                target="_blank"
                rel="noopener noreferrer"
                className="text-blue-600 underline decoration-blue-300 hover:text-blue-700 dark:text-blue-400"
              >
                {children}
              </a>
            )
          },
          hr() {
            return <hr className="my-4 border-slate-200 dark:border-gray-700" />
          },
          ...components,
        }}
      >
        {content}
      </ReactMarkdown>
    </div>
  )
})
