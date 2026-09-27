import { translate, type Locale } from './i18n'

/**
 * 顯示層格式化單一來源（spec: webui-ux-hardening）：
 * - 路徑 → 來源摘要（預設不暴露主機絕對路徑，完整值僅經明確揭露操作取用）
 * - 識別碼（UUID 等）→ 收合短碼
 * - 文件狀態列舉值 → 本地化標籤
 * 各頁面不得自行切片或猜測格式，一律由此模組輸出。
 */

/** 將來源路徑摘要為「…/父目錄/檔名」；單一段落時原樣返回（本身即為短摘要）。 */
export function describeSourcePath(path: string | null | undefined): string {
  if (!path) return ''
  const normalized = path.replace(/\\/g, '/')
  const segments = normalized.split('/').filter(Boolean)
  if (segments.length === 0) return normalized
  if (segments.length === 1) return segments[0]
  return `…/${segments.slice(-2).join('/')}`
}

/** 將長識別碼收合為前綴短碼；短於可見長度時原樣返回。 */
export function shortenIdentifier(id: string | null | undefined, visible = 8): string {
  if (!id) return ''
  const trimmed = id.trim()
  if (trimmed.length <= visible) return trimmed
  return `${trimmed.slice(0, visible)}…`
}

const STATUS_KEYS: Record<string, string> = {
  indexed: 'documents.status.indexed',
  error: 'documents.status.error',
  pending: 'documents.status.pending',
  processing: 'documents.status.processing',
}

/** 文件狀態列舉值的本地化標籤；未知列舉值原樣返回（不得猜測）。 */
export function documentStatusLabel(locale: Locale, status: string | null | undefined): string {
  if (!status) return ''
  const key = STATUS_KEYS[status]
  return key ? translate(locale, key) : status
}
