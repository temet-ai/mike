import { expect, test } from 'claude-code/testing'

import { age, commandFor, groupRows, hasNext, listUrl, pageRange, readListing, statusColor, statusWord } from '../hooks/board-data'
import type { BoardRow } from '../hooks/board-data'
import { cardOf, historyLine, savedLine, unwrap } from '../hooks/card-data'

const reply = (body: unknown) => ({ value: { status: 200, ok: true, headers: {}, text: JSON.stringify(body) } })
const row = (slug: string, status: string | null, updated: string, extra: Partial<BoardRow> = {}): BoardRow => ({
  slug,
  title: slug === 'a' ? 'Thread A' : slug,
  status,
  tldr: `tldr of ${slug} with **markdown** kept`,
  next_up: '',
  parse_error: null,
  updated_at: updated,
  ...extra,
})
const ROWS = [
  row('old-active', 'active', '2026-10-20T00:00:00Z'),
  row('new-active', 'active', '2026-10-26T00:00:00Z', { parse_error: 'no worklog.md found' }),
  row('a', 'implementing', '2026-10-25T00:00:00Z', { next_up: 'Draw the board.' }),
  row('rev', 'review', '2026-10-25T00:00:00Z'),
  row('bare', null, '2026-10-25T00:00:00Z'),
]
const NOW = Date.parse('2026-10-27T00:00:00Z')

const THREAD = { slug: 'work-threads-db', title: 'Work threads in mike', status: 'active', parse_error: null, updated_at: '2026-10-26T23:55:00Z' }
const STATE = { tldr: 'Work threads leave worklog.md.', next_up: 'Draw the card.', blockers: 'Shape unconfirmed.', open_questions: '', updated_at: '2026-10-26T23:55:00Z' }
const GET = { thread: THREAD, state: STATE, history: [{}, {}], history_total: 143, sessions: [{}, {}, {}], depends_on: ['mod-interface'], depended_on_by: ['x', 'y'] }
const SAVED = { thread: THREAD, state: STATE, created: false, state_replaced: true, entry_ord: 144 }

test('groups come in the fixed order, newest first, with no-status last', () => {
  const groups = groupRows(ROWS)
  expect(groups.map(g => g.status)).toEqual(['implementing', 'review', 'active', 'no status'])
  expect(groups[2]?.rows.map(r => r.slug)).toEqual(['new-active', 'old-active'])
})

test('status words and colours follow the spec', () => {
  expect(statusColor('implementing')).toBe('claude')
  expect(statusColor('review')).toBe('warning')
  expect(statusColor('active')).toBe('suggestion')
  expect(statusColor('parked')).toBe('inactive')
  expect(statusColor('done')).toBe('success')
  expect(statusColor(null)).toBe('subtle')
  expect(statusWord(null)).toBe('no status')
})

test('ages and paging read as words', () => {
  expect(age('2026-10-26T23:59:40Z', NOW)).toBe('just now')
  expect(age('2026-10-26T23:55:00Z', NOW)).toBe('5m ago')
  expect(age('2026-10-26T22:00:00Z', NOW)).toBe('2h ago')
  expect(age('2026-10-24T00:00:00Z', NOW)).toBe('3d ago')
  expect(age('2026-10-01T00:00:00Z', NOW)).toBe('2026-10-01')
  expect(pageRange(1, 50, 126)).toBe('51–100 of 126')
  expect(hasNext(1, 126)).toBe(true)
  expect(hasNext(2, 126)).toBe(false)
})

test('the list url encodes the root and drops the status filter for all', () => {
  expect(listUrl('http://h', '/a b', false, 1)).toBe('http://h/api/work-threads?project=%2Fa%20b&status=active,implementing,review&limit=50&offset=50')
  expect(listUrl('http://h', '/r', true, 0)).toBe('http://h/api/work-threads?project=%2Fr&limit=50&offset=0')
  expect(readListing({ threads: [{ slug: 's', title: 3 }, 'x'], total: 2 })?.rows).toHaveLength(1)
  expect(readListing({ nope: 1 })).toBeNull()
  expect(commandFor('checkpoint', 's')).toEqual({ command: 'thread:checkpoint', args: 's' })
})

test('the card reads a thread out of a bare object, a JSON string and MCP content blocks', () => {
  for (const out of [{ thread: { ...GET } }, GET, JSON.stringify(GET), [{ type: 'text', text: JSON.stringify(GET) }], { content: [{ type: 'text', text: JSON.stringify(GET) }] }]) {
    expect(unwrap(out)).not.toBeNull()
  }
  expect((unwrap({ thread: { ...GET } }) as { history_total: number }).history_total).toBe(143)
  expect(unwrap('not json')).toBeNull()
  expect(unwrap({ thread: null })).toBeNull()
})

test('the get card names history, dependencies and keeps quiet about empty fields', () => {
  const card = cardOf('mcp__mike__work_thread_get', GET)
  expect(card?.history).toBe('2 of 143 entries · 3 sessions · depends on mod-interface · needed by x, y')
  expect(card?.saved).toBeNull()
  expect(card?.blocked).toBe('Shape unconfirmed.')
  expect(card?.questions).toBe('')
  expect(historyLine({})).toBe('0 of 0 entries · 0 sessions')
})

test('the checkpoint card says what was saved', () => {
  const card = cardOf('mcp__tether__work_thread_checkpoint', SAVED)
  expect(card?.saved).toBe('checkpoint 144 · the state it replaced is kept in history')
  expect(card?.history).toBeNull()
  expect(savedLine({ created: true })).toBe('thread created')
  expect(cardOf('mcp__mike__work_thread_list', GET)).toBeNull()
})

type Ctx = Parameters<Parameters<typeof test>[1]>[0]
type OnFn = Parameters<Parameters<typeof test>[1]>[1]

function stub(on: OnFn, threads = ROWS, total = threads.length) {
  const seen = { fetched: [] as string[], closed: [] as string[], ran: [] as string[] }
  on('session.root', async () => ({ value: '/work/temet-proxy' }))
  on('clock.now', async () => ({ value: NOW }))
  on('clock.every', async () => ({ value: undefined }) as never)
  on('http.fetch', async (_, e) => {
    seen.fetched.push(e.url)
    return reply({ threads, total })
  })
  on('ui.open', async () => ({ value: { isPlaced: true } }))
  on('ui.close', async (_, e) => {
    seen.closed.push(e.id)
    return { value: undefined }
  })
  on('ui.toast', async () => ({ value: undefined }))
  on('command.run', { command: 'thread:get' }, async (_, e) => {
    seen.ran.push(e.args)
    return { text: 'shown' }
  })
  return seen
}

async function openBoard($: Ctx) {
  await $.command.run({ command: 'thread-board', args: '' })
  await new Promise(r => setTimeout(r, 50))
  return $.ui.mount({
    plugin: 'thread',
    surface: 'terminal',
    component: 'Pane',
    requestId: 'thread-board',
    props: { title: 'Threads', isFocused: true, bodyColumns: 100, placement: 'dock', scroll: { offset: 0, bodyRows: 40 }, view: {} },
  })
}

test('the pane lists the open threads grouped, with Checkpoint buttons and a fetch for this project', async ($, on) => {
  const seen = stub(on)
  const pane = await openBoard($)
  expect(seen.fetched[0]).toContain('project=%2Fwork%2Ftemet-proxy')
  expect(seen.fetched[0]).toContain('status=active,implementing,review')
  expect(await pane.find({ text: 'Open threads in temet-proxy' })).toBeDefined()
  expect(await pane.find({ text: 'implementing' })).toBeDefined()
  expect(await pane.find({ text: 'Thread A' })).toBeDefined()
  expect(await pane.find({ text: 'Draw the board.' })).toBeDefined()
  expect(await pane.find({ text: 'no worklog.md found' })).toBeDefined()
  expect(await pane.find({ text: 'not written yet' })).toBeDefined()
  expect(await pane.findAll({ type: 'Button', text: /^Checkpoint$/ })).toHaveLength(ROWS.length)
  expect(await pane.find({ text: 'New thread' })).toBeDefined()
})

test('pressing a title closes the pane and runs /thread:get with the slug', async ($, on) => {
  const seen = stub(on)
  const pane = await openBoard($)
  await pane.press({ key: 'open-rev' })
  await new Promise(r => setTimeout(r, 50))
  expect(seen.closed).toEqual(['thread-board'])
  expect(seen.ran).toEqual(['rev'])
})

test('the all switch refetches without the status filter', async ($, on) => {
  const seen = stub(on)
  const pane = await openBoard($)
  await pane.press({ key: 'all' })
  await new Promise(r => setTimeout(r, 50))
  expect(seen.fetched[1]).not.toContain('status=')
})

test('an empty project says so', async ($, on) => {
  stub(on, [])
  const pane = await openBoard($)
  expect(await pane.find({ text: 'No open threads in temet-proxy' })).toBeDefined()
})

async function mountCard($: Ctx, tool: string, output: unknown, props: Record<string, unknown> = {}) {
  return $.ui.mount({
    plugin: 'thread',
    surface: 'terminal',
    component: 'ToolResult',
    requestId: 'toolu_1',
    props: { tool_use_id: 'toolu_1', tool, output, isErrored: false, ...props },
  })
}

test('the card for work_thread_get draws the fields and no button', async ($, on) => {
  stub(on)
  const card = await mountCard($, 'mcp__mike__work_thread_get', GET)
  expect(await card.find({ text: 'Work threads in mike' })).toBeDefined()
  expect(await card.find({ text: 'Draw the card.' })).toBeDefined()
  expect(await card.find({ text: 'Shape unconfirmed.' })).toBeDefined()
  expect(await card.find({ text: /depends on mod-interface/ })).toBeDefined()
  expect(await card.find({ type: 'Button' })).toBeUndefined()
  expect(await card.find({ text: /^saved$/ })).toBeUndefined()
})

test('the card for work_thread_checkpoint (tether too) says what was saved', async ($, on) => {
  stub(on)
  const card = await mountCard($, 'mcp__tether__work_thread_checkpoint', SAVED)
  expect(await card.find({ text: /checkpoint 144/ })).toBeDefined()
  expect(await card.find({ text: /^history$/ })).toBeUndefined()
})

test('a list longer than a page draws the range and the paging buttons with valid hotkeys', async ($, on) => {
  const seen = stub(on, ROWS, 126)
  const pane = await openBoard($)
  expect(await pane.find({ text: '1–5 of 126' })).toBeDefined()
  await pane.press({ key: 'next' })
  await new Promise(r => setTimeout(r, 50))
  expect(seen.fetched[1]).toContain('offset=50')
  expect(await pane.find({ type: 'Button', text: 'Previous page' })).toBeDefined()
})
