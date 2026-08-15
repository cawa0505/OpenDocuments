import { readFileSync } from 'node:fs'

const source = readFileSync(new URL('../src/components/chat/ChatPage.tsx', import.meta.url), 'utf8')
const required = [
  'flex h-full min-h-0 flex-col',
  'flex min-h-0 w-full max-w-[860px] flex-1 flex-col',
  'min-h-0 flex-1 overflow-y-auto',
  'isStreaming && !currentStreamText',
]

const missing = required.filter((contract) => !source.includes(contract))
if (missing.length) {
  throw new Error(`Chat layout regression: missing ${missing.join(', ')}`)
}

const scrollArea = source.indexOf('min-h-0 flex-1 overflow-y-auto')
const bottomInput = source.indexOf('<div className="shrink-0">', scrollArea)
if (scrollArea < 0 || bottomInput < scrollArea) {
  throw new Error('Chat layout regression: bottom input must follow the scrollable message area')
}
