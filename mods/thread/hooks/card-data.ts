import { age, statusWord } from './board-data'

export const CARD_TOOLS = /^mcp__(mike|tether)__work_thread_(get|checkpoint)$/

export type Card = {
  kind: 'get' | 'checkpoint'
  slug: string
  title: string
  status: string
  updatedAt: string | null
  tldr: string
  next: string
  blocked: string
  questions: string
  parseError: string
  saved: string | null
  history: string | null
}

const str = (v: unknown): string => (typeof v === 'string' ? v : '')
const strs = (v: unknown): string[] => (Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : [])

/** The thread JSON inside an MCP result: an object, a JSON string, or content blocks of text. */
export function unwrap(output: unknown, depth = 0): Record<string, unknown> | null {
  if (depth > 3) return null
  if (typeof output === 'string') {
    try {
      return unwrap(JSON.parse(output), depth + 1)
    } catch {
      return null
    }
  }
  if (Array.isArray(output)) {
    for (const block of output) {
      const found = unwrap(typeof block === 'object' && block !== null ? (block as { text?: unknown }).text : block, depth + 1)
      if (found !== null) return found
    }
    return null
  }
  if (typeof output !== 'object' || output === null) return null
  const o = output as Record<string, unknown>
  if (typeof o.thread === 'object' && o.thread !== null) {
    // mike wraps the answer once more: { thread: { ...answer, thread: { slug, ... } } }
    const inner = o.thread as Record<string, unknown>
    return typeof inner.thread === 'object' && inner.thread !== null ? inner : o
  }
  if (o.content !== undefined) return unwrap(o.content, depth + 1)
  if (o.structuredContent !== undefined) return unwrap(o.structuredContent, depth + 1)
  if (typeof o.text === 'string') return unwrap(o.text, depth + 1)
  return null
}

export function savedLine(o: Record<string, unknown>): string {
  const first = o.created === true ? 'thread created' : `checkpoint ${typeof o.entry_ord === 'number' ? o.entry_ord : '?'}`
  return o.state_replaced === true ? `${first} · the state it replaced is kept in history` : first
}

export function historyLine(o: Record<string, unknown>): string {
  const history = Array.isArray(o.history) ? o.history.length : 0
  const total = typeof o.history_total === 'number' ? o.history_total : history
  const sessions = Array.isArray(o.sessions) ? o.sessions.length : 0
  const parts = [`${history} of ${total} entries`, `${sessions} sessions`]
  const on = strs(o.depends_on)
  const by = strs(o.depended_on_by)
  if (on.length > 0) parts.push(`depends on ${on.join(', ')}`)
  if (by.length > 0) parts.push(`needed by ${by.join(', ')}`)
  return parts.join(' · ')
}

export function cardOf(tool: string, output: unknown): Card | null {
  const kind = tool.endsWith('work_thread_checkpoint') ? 'checkpoint' : tool.endsWith('work_thread_get') ? 'get' : null
  if (kind === null) return null
  const o = unwrap(output)
  if (o === null) return null
  const thread = o.thread as Record<string, unknown>
  const state = typeof o.state === 'object' && o.state !== null ? (o.state as Record<string, unknown>) : {}
  const slug = str(thread.slug)
  if (slug === '') return null
  return {
    kind,
    slug,
    title: str(thread.title) || slug,
    status: statusWord(typeof thread.status === 'string' ? thread.status : null),
    updatedAt: str(state.updated_at) || str(thread.updated_at) || null,
    tldr: str(state.tldr),
    next: str(state.next_up),
    blocked: str(state.blockers),
    questions: str(state.open_questions),
    parseError: str(thread.parse_error),
    saved: kind === 'checkpoint' ? savedLine(o) : null,
    history: kind === 'get' ? historyLine(o) : null,
  }
}

export const cardAge = (c: Card, now: number): string => age(c.updatedAt, now)
