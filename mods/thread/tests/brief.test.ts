import { expect, test } from 'claude-code/testing'

import { BRIEF_LIMIT_BYTES, rowLine, tetherLoaded, threadsBrief } from '../hooks/brief'
import type { ThreadRow } from '../hooks/brief'

const SESSION = '5d1c3b5e-0d3c-4a4e-9a55-3f2a0c0de002'
const reply = (body: unknown) => ({ value: { status: 200, ok: true, headers: {}, text: JSON.stringify(body) } })

const row = (n: number): ThreadRow => ({ slug: `thread-${n}`, title: `Thread number ${n} with a fairly long title`, status: 'active' })
const rows = (count: number) => Array.from({ length: count }, (_, i) => row(i + 1))

test('a project with no open threads gets no brief', async () => {
  expect(threadsBrief({ threads: [], total: 0 })).toBeNull()
})

test('every thread is listed whole when they fit', async () => {
  const brief = threadsBrief({ threads: rows(3), total: 3 }) ?? ''
  for (const r of rows(3)) expect(brief).toContain(rowLine(r))
  expect(brief).not.toContain('more:')
})

test('a long list stays within 2KB and names how many threads it left out', async () => {
  const listing = { threads: rows(100), total: 140 }
  const brief = threadsBrief(listing) ?? ''
  expect(new TextEncoder().encode(brief).length).toBeLessThanOrEqual(BRIEF_LIMIT_BYTES)
  const shown = brief.split('\n').filter(line => line.startsWith('- thread-')).length
  expect(shown).toBeGreaterThan(0)
  expect(brief).toContain(`${140 - shown} more: work_thread_list`)
})

test('the first prompt of a session carries the brief and later prompts do not', async ($, on) => {
  const fetched: string[] = []
  on('session.id', async () => ({ value: SESSION }))
  on('session.root', async () => ({ value: '/repo' }))
  on('http.fetch', async (_, e) => {
    fetched.push(e.url)
    return reply({ threads: rows(2), total: 2 })
  })
  on('prompt.submit', async (_, e) => ({ text: e.text, context: e.context }))
  const first = await $.prompt.submit({ text: 'carry on' })
  expect(first.context?.[0]).toContain('- thread-1 [active]')
  expect(fetched[0]).toContain('/api/work-threads?project=%2Frepo&status=active%2Cimplementing%2Creview')
  const second = await $.prompt.submit({ text: 'and then' })
  expect(second.context ?? []).toHaveLength(0)
})

test('the tether mod is recognised by its folder name only', async () => {
  expect(tetherLoaded('/home/u/.claude/mods/mike:/home/u/projects/tether/mods/tether/')).toBe(true)
  expect(tetherLoaded('/home/u/.claude/mods/mike:/home/u/.claude/mods/tether-extra')).toBe(false)
  expect(tetherLoaded(undefined)).toBe(false)
})

test('with the tether mod loaded the prompt goes through untouched and mike is not asked', async ($, on) => {
  const fetched: string[] = []
  on('env.get', async (_, e) => ({ value: e.name === 'CLAUDE_CODE_PLUGIN_DIRS' ? '/m/mods/thread:/t/mods/tether' : undefined }))
  on('session.id', async () => ({ value: SESSION }))
  on('session.root', async () => ({ value: '/repo' }))
  on('http.fetch', async (_, e) => {
    fetched.push(e.url)
    return reply({ threads: rows(2), total: 2 })
  })
  on('prompt.submit', async (_, e) => ({ text: e.text, context: e.context }))
  const out = await $.prompt.submit({ text: 'carry on' })
  expect(out.context ?? []).toHaveLength(0)
  expect(fetched).toHaveLength(0)
})

test('a prompt goes through unchanged when mike does not answer', async ($, on) => {
  on('session.id', async () => ({ value: SESSION }))
  on('session.root', async () => ({ value: '/repo' }))
  on('http.fetch', async () => ({ value: { status: 502, ok: false, headers: {}, text: '' } }))
  on('prompt.submit', async (_, e) => ({ text: e.text, context: e.context }))
  const out = await $.prompt.submit({ text: 'carry on' })
  expect(out.context ?? []).toHaveLength(0)
})
