import type { ThreadRow } from './brief'

export type Focus = { slug: string; checkpointedAt: number | null }

export const THREAD_TOOL = /^mcp__(mike|tether)__work_thread_(get|checkpoint|add_entry)$/
export const SLUG_TOKEN = /^#[A-Za-z0-9._-]*$/
export const MAX_SUGGESTIONS = 8

export type Suggestion = { text: string; label: string; description?: string }

export function suggestionsFor(token: string, threads: readonly ThreadRow[]): Suggestion[] {
  const typed = token.slice(1).toLowerCase()
  return threads
    .filter(t => t.slug.toLowerCase().includes(typed))
    .sort((a, b) => Number(!a.slug.toLowerCase().startsWith(typed)) - Number(!b.slug.toLowerCase().startsWith(typed)))
    .slice(0, MAX_SUGGESTIONS)
    .map(t => ({
      text: `#${t.slug}`,
      label: `#${t.slug} [${t.status ?? 'unknown'}]`,
      ...((t.title ?? '').trim() === '' ? {} : { description: (t.title ?? '').replace(/\s+/g, ' ').trim() }),
    }))
}

export function nextFocus(current: Focus | null, tool: string, slug: unknown, now: number): Focus | null {
  const match = THREAD_TOOL.exec(tool)
  if (match === null || typeof slug !== 'string' || slug.trim() === '') return current
  const kept = current !== null && current.slug === slug ? current.checkpointedAt : null
  return { slug, checkpointedAt: match[2] === 'checkpoint' ? now : kept }
}

export function ago(ms: number): string {
  const minutes = Math.floor(ms / 60_000)
  if (minutes < 1) return 'just now'
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.floor(minutes / 60)
  return `${hours}h ${minutes % 60}m ago`
}

export function focusLine(focus: Focus | null, now: number): string | undefined {
  if (focus === null) return undefined
  const when = focus.checkpointedAt === null ? 'not checkpointed this session' : `checkpointed ${ago(now - focus.checkpointedAt)}`
  return `thread ${focus.slug} · ${when}`
}
