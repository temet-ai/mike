import { expect, test } from 'claude-code/testing'

const reply = (body: unknown) => ({ value: { status: 200, ok: true, headers: {}, text: JSON.stringify(body) } })

const SESSION = '5d1c3b5e-0d3c-4a4e-9a55-3f2a0c0de001'

const LESSONS = {
  arm: 'mod',
  lessons: [
    { id: 12, kind: 'correction', text: 'run make fmt\nbefore committing' },
    { id: 7, kind: 'convention', text: 'name a toggle by what it does' },
  ],
}

type Posted = { url: string; body: string }

test('a prompt through mike carries the lessons the proxy picked, each tagged with its id', async ($, on) => {
  const posts: Posted[] = []
  on('session.id', async () => ({ value: SESSION }))
  on('session.root', async () => ({ value: '/repo' }))
  on('env.get', async (_, e) => ({ value: e.name === 'ANTHROPIC_BASE_URL' ? 'http://localhost:9988' : undefined }))
  on('http.fetch', async (_, e) => {
    posts.push({ url: e.url, body: String(e.init?.body ?? '') })
    return reply(LESSONS)
  })
  on('prompt.submit', async (_, e) => ({ text: e.text, context: e.context }))
  const out = await $.prompt.submit({ text: 'fix the build' })
  const context = out.context ?? []
  expect(context).toHaveLength(1)
  expect(context[0]).toContain('[lesson:12] [correction] run make fmt before committing')
  expect(context[0]).toContain('[lesson:7] [convention] name a toggle by what it does')
  expect(posts[0].url).toBe('http://127.0.0.1:9989/api/lessons/for-prompt')
  expect(JSON.parse(posts[0].body)).toMatchObject({ session: SESSION, project: '/repo', prompt: 'fix the build', held: [] })
})

test('lessons already added are reported as held on the next prompt', async ($, on) => {
  const posts: Posted[] = []
  on('session.id', async () => ({ value: SESSION }))
  on('session.root', async () => ({ value: '/repo' }))
  on('env.get', async (_, e) => ({ value: e.name === 'ANTHROPIC_BASE_URL' ? 'http://localhost:9988' : undefined }))
  on('http.fetch', async (_, e) => {
    posts.push({ url: e.url, body: String(e.init?.body ?? '') })
    return reply(LESSONS)
  })
  on('prompt.submit', async (_, e) => ({ text: e.text, context: e.context }))
  await $.prompt.submit({ text: 'one' })
  await $.prompt.submit({ text: 'two' })
  expect(JSON.parse(posts[1].body).held).toEqual([12, 7])
})

test('the control arm, the toggle off or no lessons add nothing', async ($, on) => {
  const answers: unknown[] = [
    { arm: 'control', lessons: [] },
    { arm: 'off', lessons: [] },
    { arm: 'mod', lessons: [] },
    { lines: [], tail: true },
  ]
  on('session.id', async () => ({ value: SESSION }))
  on('session.root', async () => ({ value: '/repo' }))
  on('env.get', async (_, e) => ({ value: e.name === 'ANTHROPIC_BASE_URL' ? 'http://localhost:9988' : undefined }))
  on('http.fetch', async () => reply(answers.shift()))
  on('prompt.submit', async (_, e) => ({ text: e.text, context: e.context }))
  for (const text of ['a', 'b', 'c', 'd']) {
    const out = await $.prompt.submit({ text })
    expect(out.context ?? []).toHaveLength(0)
  }
})

test('a session not going through mike is not asked for lessons', async ($, on) => {
  const asked: string[] = []
  on('session.id', async () => ({ value: 'self-session' }))
  on('http.fetch', async (_, e) => {
    asked.push(e.url)
    return reply({ lines: [], tail: true })
  })
  on('prompt.submit', async (_, e) => ({ text: e.text, context: e.context }))
  await $.prompt.submit({ text: 'hello' })
  expect(asked.filter(u => u.includes('/api/lessons/'))).toHaveLength(0)
})

test('mike not answering leaves the prompt as typed', async ($, on) => {
  on('session.id', async () => ({ value: SESSION }))
  on('session.root', async () => ({ value: '/repo' }))
  on('env.get', async (_, e) => ({ value: e.name === 'ANTHROPIC_BASE_URL' ? 'http://localhost:9988' : undefined }))
  on('http.fetch', async () => {
    throw new Error('connection refused')
  })
  on('prompt.submit', async (_, e) => ({ text: e.text, context: e.context }))
  const out = await $.prompt.submit({ text: 'hello' })
  expect(out.text).toBe('hello')
  expect(out.context ?? []).toHaveLength(0)
})


test('compaction rearms versioned lessons and procedure catalogs without model calls', async ($, on) => {
  const posts: Posted[] = []
  on('session.id', async () => ({ value: SESSION }))
  on('session.root', async () => ({ value: '/repo' }))
  on('env.get', async (_, e) => ({ value: e.name === 'ANTHROPIC_BASE_URL' ? 'http://localhost:9988' : undefined }))
  on('http.fetch', async (_, e) => {
    posts.push({ url: e.url, body: String(e.init?.body ?? '') })
    return reply({ arm: 'mod', lessons: [{ id: 12, kind: 'correction', text: 'current', revision: 'v2' }], topic_catalog: 'Load database-migrations for migration review' })
  })
  on('prompt.submit', async (_, e) => ({ text: e.text, context: e.context }))
  const summary = [{ role: 'user' as const, text: 'Compacted working context', toolUses: [], toolResults: [] }]
  on('session.compact', async () => ({ messages: summary }))
  const first = await $.prompt.submit({ text: 'review migration' })
  expect(first.context?.[0]).toContain('[revision:v2]')
  expect(first.context?.[0]).toContain('database-migrations')
  const second = await $.prompt.submit({ text: 'continue' })
  expect(second.context?.[0]).not.toContain('database-migrations')
  expect(JSON.parse(posts[1].body).held_versions).toEqual({ '12': 'v2' })
  await $.session.compact({ messages: summary, trigger: 'manual' })
  const third = await $.prompt.submit({ text: 'continue after compaction' })
  expect(JSON.parse(posts[2].body)).toMatchObject({ held: [], held_versions: {}, refresh: true })
  expect(third.context?.[0]).toContain('database-migrations')
})

const KEEPER = '<system-reminder source="memory-keeper">\n[keeper:abc123def456]\nthread release-mike: open\n</system-reminder>'

test('a lesson-control session still gets the memory-keeper card', async ($, on) => {
  on('session.id', async () => ({ value: SESSION }))
  on('session.root', async () => ({ value: '/repo' }))
  on('env.get', async (_, e) => ({ value: e.name === 'ANTHROPIC_BASE_URL' ? 'http://localhost:9988' : undefined }))
  on('http.fetch', async () => reply({ arm: 'control', lessons: [], memory_keeper: KEEPER, memory_keeper_hash: 'abc123def456' }))
  on('prompt.submit', async (_, e) => ({ text: e.text, context: e.context }))
  const out = await $.prompt.submit({ text: 'continue with the release' })
  expect(out.context ?? []).toEqual([KEEPER])
})

test('the same memory-keeper card is added once until compaction', async ($, on) => {
  on('session.id', async () => ({ value: SESSION }))
  on('session.root', async () => ({ value: '/repo' }))
  on('env.get', async (_, e) => ({ value: e.name === 'ANTHROPIC_BASE_URL' ? 'http://localhost:9988' : undefined }))
  on('http.fetch', async () => reply({ ...LESSONS, memory_keeper: KEEPER, memory_keeper_hash: 'abc123def456' }))
  on('prompt.submit', async (_, e) => ({ text: e.text, context: e.context }))
  const summary = [{ role: 'user' as const, text: 'Compacted working context', toolUses: [], toolResults: [] }]
  on('session.compact', async () => ({ messages: summary }))
  const first = await $.prompt.submit({ text: 'one' })
  expect(first.context?.[0]).toContain('[keeper:abc123def456]')
  expect(first.context?.[0]).toContain('[lesson:12]')
  const second = await $.prompt.submit({ text: 'two' })
  expect(second.context?.[0] ?? '').not.toContain('[keeper:')
  await $.session.compact({ messages: summary, trigger: 'manual' })
  const third = await $.prompt.submit({ text: 'three' })
  expect(third.context?.[0]).toContain('[keeper:abc123def456]')
})
