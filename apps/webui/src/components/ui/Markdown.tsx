import ReactMarkdown from 'react-markdown'
import rehypeHighlight from 'rehype-highlight'
import { memo } from 'react'
import type { Components } from 'react-markdown'

interface MarkdownProps {
  content: string
  className?: string
  /** 自訂節點渲染器（例如 citation 互動標籤） */
  components?: Components
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
    <div className={className}>
      <ReactMarkdown
        rehypePlugins={[rehypeHighlight]}
        components={{
          // 行內 code vs 區塊 code
          code({ className: cls, children, ...props }) {
            const isBlock = /language-/.test(cls || '')
            if (isBlock) {
              return (
                <code className={cls} {...props}>
                  {children}
                </code>
              )
            }
            return (
              <code
                className="rounded bg-slate-100 px-1.5 py-0.5 font-mono text-[0.85em] text-blue-700 dark:bg-gray-800 dark:text-blue-300"
                {...props}
              >
                {children}
              </code>
            )
          },
          pre({ children }) {
            return (
              <pre className="overflow-x-auto rounded-md border border-slate-200 bg-slate-950 p-3 text-[13px] leading-6 text-slate-100 dark:border-gray-700">
                {children}
              </pre>
            )
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
