import { expect, test } from 'claude-code/testing'

import { ago, focusLine, nextFocus, suggestionsFor } from '../hooks/focus'
import type { ThreadRow } from '../hooks/brief'

const THREADS: ThreadRow[] = [
  { slug: 'release-mike', title: 'Release mike', status: 'active' },
  { slug: 'mike-release-notes', title: null, status: 'parked' },
  { slug: 'frontend-motion', title: 'Frontend motion', status: 'implementing' },
]
const reply = (body: unknown) => ({ value: { status: 200, ok: true, headers: {}, text: JSON.stringify(body) } })

test('a #token suggests matching threads, those starting with it first', async () => {
  const rows = suggestionsFor('#rel', THREADS)
  expect(rows.map(r => r.text)).toEqual(['#release-mike', '#mike-release-notes'])
  expect(rows[0]?.label).toBe('#release-mike [active]')
  expect(rows[0]?.description).toBe('Release mike')
  expect(rows[1]?.description).toBeUndefined()
})

test('a bare # suggests every thread up to the limit', async () => {
  expect(suggestionsFor('#', THREADS)).toHaveLength(3)
})

test('reading a thread focuses it, a checkpoint stamps it, another thread resets the stamp', async () => {
  const read = nextFocus(null, 'mcp__mike__work_thread_get', 'release-mike', 1000)
  expect(read).toEqual({ slug: 'release-mike', checkpointedAt: null })
  const saved = nextFocus(read, 'mcp__mike__work_thread_checkpoint', 'release-mike', 5000)
  expect(saved).toEqual({ slug: 'release-mike', checkpointedAt: 5000 })
  expect(nextFocus(saved, 'mcp__mike__work_thread_get', 'release-mike', 9000)).toEqual(saved)
  expect(nextFocus(saved, 'mcp__tether__work_thread_get', 'frontend-motion', 9000)).toEqual({ slug: 'frontend-motion', checkpointedAt: null })
  expect(nextFocus(saved, 'mcp__mike__work_thread_list', undefined, 9000)).toEqual(saved)
})

test('the status line says which thread and how long since its checkpoint', async () => {
  expect(focusLine(null, 0)).toBeUndefined()
  expect(focusLine({ slug: 's', checkpointedAt: null }, 0)).toBe('thread s · not checkpointed this session')
  expect(focusLine({ slug: 's', checkpointedAt: 0 }, 12 * 60_000)).toBe('thread s · checkpointed 12m ago')
  expect(ago(30_000)).toBe('just now')
  expect(ago(125 * 60_000)).toBe('2h 5m ago')
})

test('typing #fro in the prompt box offers the frontend-motion thread from mike', async ($, on) => {
  const fetched: string[] = []
  on('session.root', async () => ({ value: '/repo' }))
  on('http.fetch', async (_, e) => {
    fetched.push(e.url)
    return reply({ threads: THREADS, total: 3 })
  })
  on('prompt.autocomplete', async () => ({ suggestions: [] }))
  on('clock.now', async () => ({ value: 1_000_000 }))
  const out = await $.prompt.autocomplete({ text: 'carry on #fro', cursor: 13, token: '#fro', start: 9 })
  expect(out.suggestions.map(s => s.text)).toEqual(['#frontend-motion'])
  expect(fetched[0]).toContain('/api/work-threads?project=%2Frepo')
})
