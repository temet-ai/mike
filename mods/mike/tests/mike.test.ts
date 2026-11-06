import type { CommandRunInput, RenderPropsOf } from 'claude-code'
import { expect, test } from 'claude-code/testing'

import { goesThroughMike } from '../hooks/register'

const run = (command: string, args: string) => ({ command, args }) as CommandRunInput

const TURNS = {
  degraded: false,
  turns: [
    { session_id: 'bd24c5ee-70af', user_prompt: 'why is the seat idle?', final_assistant_text: 'Because nobody pressed Enter.' },
  ],
}

test('/recall lists the matching past turns', async ($, on) => {
  on('session.root', async () => ({ value: '/repo' }))
  const asked: string[] = []
  on('http.fetch', async ($, e) => {
    asked.push(e.url)
    return { value: { status: 200, ok: true, headers: {}, text: JSON.stringify(TURNS) } }
  })
  const out = await $.command.run(run('recall', 'idle seat'))
  expect(asked[0]).toBe('http://127.0.0.1:9989/api/recall/similar-turns')
  expect(out.text).toContain('why is the seat idle?')
  expect(out.text).toContain('[bd24c5ee]')
})

test('/recall says so when mike does not answer', async ($, on) => {
  on('session.root', async () => ({ value: '/repo' }))
  on('http.fetch', async () => {
    throw new Error('connection refused')
  })
  const out = await $.command.run(run('recall', 'anything'))
  expect(out.text).toContain('mike did not answer')
})

test('/recall with no words asks for some', async $ => {
  const out = await $.command.run(run('recall', '  '))
  expect(out.text).toContain('Usage')
})

test('unread mike mail shows on the status line', async ($, on) => {
  const status: (string | undefined)[] = []
  on('session.id', async () => ({ value: 'self-session' }))
  on('session.root', async () => ({ value: '/repo' }))
  on('clock.now', async () => ({ value: 0 }))
  on('clock.every', async () => ({ value: undefined }))
  on('command.register', async ($, e) => ({ value: { command: e.name } }))
  on('session.start', async () => ({ cwd: '/repo' }))
  on('ui.log', async () => ({ value: undefined }))
  on('ui.toast', async () => ({ value: undefined }))
  on('ui.status', async ($, e) => {
    status.push(e.text)
    return { value: undefined }
  })
  on('http.fetch', async ($, e) => {
    const body = e.url.endsWith('/messages')
      ? { inbox: [{ from_session_uuid: '77009065-1dfb', consumed_at: null, dead_lettered_at: null }] }
      : { roster: [] }
    return { value: { status: 200, ok: true, headers: {}, text: JSON.stringify(body) } }
  })
  await $.session.start({ cwd: '/repo', surface: 'terminal', isInteractive: true })
  for (let i = 0; i < 200 && status.length === 0; i++) await Promise.resolve()
  expect(status).toContain('1 unread from 77009065')
})

test('the sessions pane lists each session, this one marked', async ($, on) => {
  on('session.id', async () => ({ value: 'aaaaaaaa-self' }))
  on('session.root', async () => ({ value: '/repo' }))
  on('clock.now', async () => ({ value: 0 }))
  on('ui.open', async () => ({ value: { isPlaced: true as const } }))
  on('http.fetch', async () => ({
    value: {
      status: 200,
      ok: true,
      headers: {},
      text: JSON.stringify({
        roster: [
          { session_id: 'aaaaaaaa-self', name: null, role: null, liveness: 'live', busy: true, session_state: 'running', goal: null },
          { session_id: 'bbbbbbbb-peer', name: 'mike-fleet-lead', role: 'fleet-lead', liveness: 'live', busy: false, session_state: 'idle', goal: 'fleet TUI' },
        ],
      }),
    },
  }))
  await $.command.run(run('seats', ''))
  for (const surface of ['terminal', 'desktop'] as const) {
    const ui = await $.ui.mount({ plugin: 'mike', surface, component: 'Pane', requestId: 'mike-seats-pane', props: { title: 'mike sessions' } as RenderPropsOf['Pane'] })
    expect(await ui.find({ type: 'Text', text: /2 sessions/ })).toBeDefined()
    expect(await ui.find({ type: 'Text', text: /mike-fleet-lead · fleet-lead/ })).toBeDefined()
    expect(await ui.find({ type: 'Text', text: /\(this session\)/ })).toBeDefined()
    await ui.unmount()
  }
})

const reply = (body: unknown) => ({ value: { status: 200, ok: true, headers: {}, text: JSON.stringify(body) } })

test('a prompt carries the lines others changed, once, at most twelve', async ($, on) => {
  const asked: string[] = []
  const lines = Array.from({ length: 15 }, (_, n) => `ann edited src/f${n}.rs`)
  const queue = [{ lines, tail: true }, { lines: [], tail: true }]
  on('session.id', async () => ({ value: 'self-session' }))
  on('http.fetch', async ($, e) => {
    asked.push(e.url)
    return reply(queue.shift() ?? { lines: [], tail: true })
  })
  on('prompt.submit', async (_, e) => ({ text: e.text, context: e.context }))
  const first = await $.prompt.submit({ text: 'hello' })
  expect(asked[0]).toBe('http://127.0.0.1:9989/api/changes?view=tail&session=self-session')
  const context = first.context ?? []
  expect(context).toHaveLength(1)
  expect(context[0]).toContain('What others changed since you last looked:')
  expect(context[0].split('\n').filter(l => l.startsWith('- '))).toHaveLength(12)
  const second = await $.prompt.submit({ text: 'again' })
  expect(second.context ?? []).toHaveLength(0)
})

test('a session that goes through mike gets the lines from the proxy, not from the mod', async ($, on) => {
  const asked: string[] = []
  on('session.id', async () => ({ value: 'self-session' }))
  on('env.get', async (_, e) => ({ value: e.name === 'HTTPS_PROXY' ? 'http://127.0.0.1:9987' : undefined }))
  on('http.fetch', async ($, e) => {
    asked.push(e.url)
    return reply({ lines: ['ann edited src/a.rs'], tail: true })
  })
  on('prompt.submit', async (_, e) => ({ text: e.text, context: e.context }))
  const out = await $.prompt.submit({ text: 'hello' })
  expect(out.context ?? []).toHaveLength(0)
  expect(asked.filter(u => u.includes('view=tail'))).toHaveLength(0)
})

test('only mike addresses count as going through mike', () => {
  expect(goesThroughMike('http://localhost:9988', undefined)).toBe(true)
  expect(goesThroughMike(undefined, 'http://127.0.0.1:9987')).toBe(true)
  expect(goesThroughMike('https://api.anthropic.com', 'http://corp-proxy:8080')).toBe(false)
  expect(goesThroughMike('http://localhost:99881', undefined)).toBe(false)
  expect(goesThroughMike(undefined, undefined)).toBe(false)
})

test('a prompt with nothing changed, the toggle off or mike silent adds nothing', async ($, on) => {
  const bodies: unknown[] = [{ lines: [], tail: true }, { lines: ['x'], tail: false }]
  on('session.id', async () => ({ value: 'self-session' }))
  on('http.fetch', async () => {
    const body = bodies.shift()
    if (body === undefined) throw new Error('connection refused')
    return reply(body)
  })
  on('prompt.submit', async (_, e) => ({ text: e.text, context: e.context }))
  for (const text of ['one', 'two', 'three']) {
    const out = await $.prompt.submit({ text })
    expect(out.text).toBe(text)
    expect(out.context ?? []).toHaveLength(0)
  }
})

test('after the session id changes, mail and changes are read under the new id', async ($, on) => {
  let id = 'old-session'
  const asked: string[] = []
  on('session.id', async () => ({ value: id }))
  on('http.fetch', async ($, e) => {
    asked.push(e.url)
    return reply({ lines: [], tail: true })
  })
  on('prompt.submit', async (_, e) => ({ text: e.text, context: e.context }))
  await $.prompt.submit({ text: 'before' })
  id = 'new-session'
  await $.prompt.submit({ text: 'after' })
  expect(asked[0]).toContain('session=old-session')
  expect(asked[1]).toContain('session=new-session')
})

test('the status line joins unread mail and the changes summary', async ($, on) => {
  const status: (string | undefined)[] = []
  on('session.id', async () => ({ value: 'self-session' }))
  on('session.root', async () => ({ value: '/repo' }))
  on('clock.now', async () => ({ value: 0 }))
  on('clock.every', async () => ({ value: undefined }))
  on('command.register', async ($, e) => ({ value: { command: e.name } }))
  on('session.start', async () => ({ cwd: '/repo' }))
  on('ui.log', async () => ({ value: undefined }))
  on('ui.toast', async () => ({ value: undefined }))
  on('ui.status', async ($, e) => {
    status.push(e.text)
    return { value: undefined }
  })
  on('http.fetch', async ($, e) => {
    if (e.url.includes('view=standing')) return reply({ uncommitted: ['a.rs', 'b.rs'], events_waiting: 3, events_waiting_capped: false })
    if (e.url.endsWith('/messages')) return reply({ inbox: [{ from_session_uuid: '77009065-1dfb', consumed_at: null, dead_lettered_at: null }] })
    return reply({ roster: [] })
  })
  await $.session.start({ cwd: '/repo', surface: 'terminal', isInteractive: true })
  for (let i = 0; i < 300 && !status.some(s => s?.includes('unread') && s.includes('changes by others')); i++) await Promise.resolve()
  expect(status).toContain('1 unread from 77009065 · 3 changes by others, 2 uncommitted')
})

test('at session start the mods this session loaded are posted to mike as a mods note', async ($, on) => {
  const posts: { url: string; body: string }[] = []
  on('session.id', async () => ({ value: 'self-session' }))
  on('session.root', async () => ({ value: '/repo' }))
  on('clock.now', async () => ({ value: 0 }))
  on('clock.every', async () => ({ value: undefined }))
  on('command.register', async ($, e) => ({ value: { command: e.name } }))
  on('session.start', async () => ({ cwd: '/repo' }))
  on('ui.log', async () => ({ value: undefined }))
  on('ui.toast', async () => ({ value: undefined }))
  on('ui.status', async () => ({ value: undefined }))
  on('env.get', async (_$, e) =>
    e.name === 'CLAUDE_CODE_PLUGIN_DIRS' ? { value: '/m/mods:/m/plain' } : { value: undefined },
  )
  on('fs.stat', async () => ({ value: { isLink: false } }))
  on('fs.list', async () => ({ value: [] }))
  on('fs.exists', async () => ({ value: false }))
  on('fs.read', async (_$, e, next) => {
    const files: Record<string, string> = {
      '/m/mods/.claude-plugin/plugin.json': JSON.stringify({ name: 'mods', version: '0.2.0' }),
      '/m/mods/.claude-plugin/mod.json': JSON.stringify({ tagline: 'Lists mods', offers: { commands: ['/mods'] } }),
    }
    return e.path in files ? { value: files[e.path] } : next(e)
  })
  on('http.fetch', async ($, e) => {
    if (e.init?.method === 'POST') posts.push({ url: e.url, body: String(e.init.body) })
    return reply(e.url.endsWith('/messages') ? { inbox: [] } : e.url.includes('/roster') ? { roster: [] } : {})
  })
  await $.session.start({ cwd: '/repo', surface: 'terminal', isInteractive: true })
  for (let i = 0; i < 400 && posts.length === 0; i++) await Promise.resolve()
  expect(posts).toHaveLength(1)
  expect(posts[0].url).toBe('http://127.0.0.1:9989/api/sessions/self-session/notes')
  const note = JSON.parse(posts[0].body)
  expect(note.kind).toBe('mods')
  expect(note.data.entries.map((m: { name: string }) => m.name)).toEqual(['mods', 'plain'])
  expect(note.data.entries[0]).toMatchObject({ version: '0.2.0', tagline: 'Lists mods', scope: 'global' })
})

test('under tether the mod does nothing: tether serves /seats, /recall, prompts and the loaded-mods note', async ($, on) => {
  const asked: { url: string; body?: string }[] = []
  const tetherDirs = '/home/u/.claude/mods/mike:/home/u/.claude/mods/tether'
  on('session.id', async () => ({ value: 'self-session' }))
  on('session.root', async () => ({ value: '/repo' }))
  on('env.get', async (_, e) => ({ value: e.name === 'CLAUDE_CODE_PLUGIN_DIRS' ? tetherDirs : undefined }))
  on('fs.exists', async () => ({ value: false }))
  on('fs.list', async () => ({ value: [] }))
  on('fs.stat', async () => ({ value: undefined }))
  on('fs.read', async () => { throw new Error('absent') })
  on('command.register', async () => ({ value: undefined }))
  on('session.start', async (_, e: any) => ({ cwd: e.cwd }))
  on('http.fetch', async ($, e) => {
    asked.push({ url: e.url, body: e.init?.body as string | undefined })
    return reply({ lines: ['ann edited src/a.rs'], tail: true })
  })
  on('prompt.submit', async (_, e) => ({ text: e.text, context: e.context }))
  on('command.run', async () => ({ text: 'tether answered' }))
  await $.session.start({ cwd: '/repo', surface: 'terminal', isInteractive: true } as any)
  for (let i = 0; i < 20; i++) await new Promise(r => setTimeout(r, 5))
  expect(asked).toHaveLength(0)
  const out = await $.prompt.submit({ text: 'hello' })
  expect(out.context ?? []).toHaveLength(0)
  expect((await $.command.run(run('seats', ''))).text).toBe('tether answered')
  expect((await $.command.run(run('recall', 'x'))).text).toBe('tether answered')
  expect(asked).toHaveLength(0)
})
