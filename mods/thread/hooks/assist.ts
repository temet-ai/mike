import type { EngineInterface, Register } from 'claude-code'

import { isListing } from './brief'
import type { ThreadRow } from './brief'
import { SLUG_TOKEN, focusLine, nextFocus, suggestionsFor } from './focus'
import type { Focus } from './focus'

const INSPECT = 'http://127.0.0.1:9989'
const LIST_TTL_MS = 60_000

let threads: ThreadRow[] = []
let listedAt = -Infinity
let focus: Focus | null = null

function quiet() {}

async function refreshThreads($: EngineInterface): Promise<void> {
  const now = await $.clock.now()
  if (now - listedAt < LIST_TTL_MS) return
  listedAt = now
  const root = await $.session.root()
  try {
    const res = await $.http.fetch(`${INSPECT}/api/work-threads?project=${encodeURIComponent(root)}&limit=500`)
    if (!res.ok) return
    const body: unknown = JSON.parse(res.text)
    if (isListing(body)) threads = body.threads
  } catch {
    return
  }
}

async function showFocus($: EngineInterface): Promise<void> {
  $.ui.status(focusLine(focus, await $.clock.now()))
}

export function startAssist($: EngineInterface): void {
  focus = null
  listedAt = -Infinity
  $.clock.every(60_000, () => showFocus($).catch(quiet))
}

export const register: Register = on => {
  on('prompt.autocomplete', { token: SLUG_TOKEN }, async ($, e, next) => {
    const before = await next(e)
    await refreshThreads($)
    return { suggestions: [...before.suggestions, ...suggestionsFor(e.token, threads)] }
  })

  on('tool.call', { tool: /work_thread_/ }, async ($, e, next) => {
    const result = await next(e)
    if (e.agentId !== undefined || 'deny' in result) return result
    if ('isError' in result && result.isError === true) return result
    const slug = (e as unknown as { slug?: unknown }).slug
    focus = nextFocus(focus, e.tool, slug, await $.clock.now())
    if (e.tool.endsWith('work_thread_checkpoint')) listedAt = -Infinity
    await showFocus($)
    return result
  })
}
