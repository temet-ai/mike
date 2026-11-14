import { atom, read, update } from 'claude-code'
import type { EngineInterface, HttpInit, Register } from 'claude-code'

import type { Roster, Seat } from '../types'
import { entryOf, modsNoteBody, pluginDirsOf } from './loadedMods'
import type { ModEntry, ModListing } from './loadedMods'
import { isDelivery, keeperBlock, lessonsBlock, wantsLessons } from './lessons'

const INSPECT = 'http://127.0.0.1:9989'
const PANE = 'mike-seats-pane'
const roster = atom({ plugin: 'mike', key: 'roster' } as const, { seats: [], at: '', error: null })

type RosterRow = {
  session_id: string
  name: string | null
  role: string | null
  liveness: string
  busy: boolean
  session_state: string | null
  goal: string | null
}
type InboxRow = { from_session_uuid: string; consumed_at: string | null; dead_lettered_at: string | null }
type Turn = { session_id: string; user_prompt: string | null; final_assistant_text: string | null }

const clip = (text: string, n: number) => {
  const one = text.replace(/\s+/g, ' ').trim()
  return one.length > n ? one.slice(0, n - 1) + '…' : one
}

const MAX_CHANGE_LINES = 12
const CHANGES_HEADING = 'What others changed since you last looked:'

let unread = 0
const heldLessons = new Map<number, string>()
let refreshLessons = false
let lessonSession: string | undefined
const heldCatalogs = new Set<string>()
const heldKeeper = new Set<string>()
let inboxText: string | undefined
let changesText: string | undefined

function quiet() {}

function showStatus($: EngineInterface) {
  const parts = [inboxText, changesText].filter((p): p is string => p !== undefined)
  $.ui.status(parts.length === 0 ? undefined : parts.join(' · '))
}

function changesBlock(lines: readonly string[]): string | null {
  const shown = lines.filter(l => l.trim() !== '').slice(0, MAX_CHANGE_LINES)
  if (shown.length === 0) return null
  return `<system-reminder>\n${CHANGES_HEADING}\n${shown.map(l => '- ' + l).join('\n')}\n</system-reminder>`
}

export function goesThroughMike(baseUrl: string | undefined, httpsProxy: string | undefined): boolean {
  return /:9988(\/|$)/.test(baseUrl ?? '') || /:9987(\/|$)/.test(httpsProxy ?? '')
}

async function proxiedByMike($: EngineInterface): Promise<boolean> {
  try {
    return goesThroughMike(await $.env.get('ANTHROPIC_BASE_URL'), await $.env.get('HTTPS_PROXY'))
  } catch {
    return false
  }
}

async function lessonContext($: EngineInterface, text: string): Promise<string | null> {
  if (!(await proxiedByMike($))) return null
  const session = await $.session.id()
  if (session !== lessonSession) {
    heldLessons.clear()
    heldCatalogs.clear()
    heldKeeper.clear()
    refreshLessons = true
    lessonSession = session
  }
  const body = await fetchJson<unknown>($, '/api/lessons/for-prompt', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      session,
      project: await $.session.root(),
      prompt: text,
      held: [...heldLessons.keys()],
      held_versions: Object.fromEntries(heldLessons),
      refresh: refreshLessons,
    }),
  })
  if (!isDelivery(body)) return null
  const keeper = keeperBlock(body, heldKeeper)
  if (body.arm !== 'mod') return keeper
  for (const lesson of body.lessons) heldLessons.set(lesson.id, lesson.revision ?? 'legacy')
  refreshLessons = false
  const blocks = [lessonsBlock(body.lessons)]
  if (body.topic_catalog && !heldCatalogs.has(body.topic_catalog)) {
    heldCatalogs.add(body.topic_catalog)
    blocks.push(body.topic_catalog)
  }
  blocks.push(keeper)
  return blocks.filter(Boolean).join('\n') || null
}

async function takeChangeLines($: EngineInterface): Promise<string[]> {
  const self = await $.session.id()
  const body = await fetchJson<{ lines: string[]; tail: boolean }>(
    $,
    '/api/changes?view=tail&session=' + encodeURIComponent(self),
  )
  if (body === null || body.tail !== true || !Array.isArray(body.lines)) return []
  return body.lines
}

async function refreshChangeSummary($: EngineInterface) {
  const self = await $.session.id()
  const body = await fetchJson<{ uncommitted?: string[]; events_waiting?: number; events_waiting_capped?: boolean }>(
    $,
    '/api/changes?view=standing&session=' + encodeURIComponent(self),
  )
  if (body === null) {
    changesText = undefined
    return showStatus($)
  }
  const waiting = body.events_waiting ?? 0
  const mine = body.uncommitted?.length ?? 0
  const bits: string[] = []
  if (waiting > 0) bits.push(`${waiting}${body.events_waiting_capped ? '+' : ''} changes by others`)
  if (mine > 0) bits.push(`${mine} uncommitted`)
  const code = await fetchJson<{ session_counts?: { open?: number } | null }>(
    $,
    '/api/code-findings?hours=24&limit=1&session=' + encodeURIComponent(self),
  )
  const open = code?.session_counts?.open ?? 0
  if (open > 0) bits.push(`${open} code finding${open === 1 ? '' : 's'} open`)
  changesText = bits.length === 0 ? undefined : bits.join(', ')
  showStatus($)
}

async function fetchJson<T>($: EngineInterface, path: string, init?: HttpInit): Promise<T | null> {
  try {
    const res = await $.http.fetch(INSPECT + path, init)
    return res.ok ? (JSON.parse(res.text) as T) : null
  } catch {
    return null
  }
}

async function refreshRoster($: EngineInterface) {
  const self = await $.session.id()
  const root = await $.session.root()
  const body = await fetchJson<{ roster: RosterRow[] }>(
    $,
    '/api/workspace/projects/' + encodeURIComponent(root) + '/roster',
  )
  const at = new Date(await $.clock.now()).toISOString().slice(11, 19)
  if (body === null) {
    await update($, roster, old => ({ ...old, at, error: 'mike did not answer on ' + INSPECT }))
    return
  }
  const seats: Seat[] = body.roster.map(row => ({
    id: row.session_id,
    label: row.name ?? row.session_id.slice(0, 8),
    role: row.role,
    liveness: row.liveness,
    state: row.busy ? 'working' : (row.session_state ?? 'idle'),
    goal: row.goal,
    isSelf: row.session_id === self,
  }))
  await update($, roster, () => ({ seats, at, error: null }))
}

async function checkInbox($: EngineInterface) {
  const self = await $.session.id()
  const body = await fetchJson<{ inbox: InboxRow[] }>($, '/api/sessions/' + self + '/messages')
  if (body === null) return
  const waiting = body.inbox.filter(m => m.consumed_at === null && m.dead_lettered_at === null)
  if (waiting.length > unread) $.ui.toast(`${waiting.length} unread mike message(s)`)
  unread = waiting.length
  const first = waiting[0]
  inboxText = first === undefined ? undefined : `${unread} unread from ${first.from_session_uuid.slice(0, 8)}`
  showStatus($)
}

async function readModEntry($: EngineInterface, folder: string, scope: ModEntry['scope']): Promise<ModEntry> {
  const stat = await $.fs.stat(folder, { resolve: true }).catch(() => undefined)
  const manifest = await $.fs.read(`${folder}/.claude-plugin/plugin.json`).catch(() => undefined)
  const self = await $.fs.read(`${folder}/.claude-plugin/mod.json`).catch(() => undefined)
  return entryOf(folder, manifest, self, stat?.isLink ? stat.realPath : undefined, scope)
}

async function skillMods($: EngineInterface, skills: string | undefined): Promise<string[]> {
  if (!skills) return []
  const children = await $.fs.list(skills).catch(() => [])
  const folders = children.map(child => `${skills}/${child.name}`)
  const isMod = await Promise.all(
    folders.map(folder => $.fs.exists(`${folder}/.claude-plugin/plugin.json`).catch(() => false)),
  )
  return folders.filter((_, i) => isMod[i])
}

async function listLoadedMods($: EngineInterface): Promise<ModListing> {
  const home = await $.env.get('HOME')
  const root = await $.session.root().catch(() => undefined)
  const globalDirs = [
    ...pluginDirsOf(await $.env.get('CLAUDE_CODE_PLUGIN_DIRS')),
    ...(await skillMods($, home ? `${home}/.claude/skills` : undefined)),
  ]
  const projectDirs = root && root !== home ? await skillMods($, `${root}/.claude/skills`) : []
  const entries = [
    ...(await Promise.all(projectDirs.map(dir => readModEntry($, dir, 'project')))),
    ...(await Promise.all(globalDirs.map(dir => readModEntry($, dir, 'global')))),
  ].filter((entry, i, all) => all.findIndex(other => other.name === entry.name) === i)
  return { entries, home, project: root?.split('/').filter(Boolean).pop() }
}

type PostJson = (
  path: string,
  init: { method: string; headers: Record<string, string>; body: string },
) => Promise<unknown>

async function postLoadedMods($: EngineInterface, post: PostJson): Promise<void> {
  const self = await $.session.id()
  const listing = await listLoadedMods($)
  await post(`/api/sessions/${encodeURIComponent(self)}/notes`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(modsNoteBody(listing)),
  })
}

const COMMANDS = [
  { name: 'seats', description: "Show this project's mike sessions in a pane" },
  { name: 'recall', description: "Search the operator's past turns in mike", argumentHint: '<what to look for>' },
]

/** True when the tether mod is among the session's mod folders: tether then
 * serves /seats, /recall, lessons, the changes feed and the status line, and
 * tells mike which mods the session loaded, so this mod does nothing. */
export function tetherLoaded(pluginDirs: string | undefined): boolean {
  return pluginDirsOf(pluginDirs).some(dir => dir.split('/').pop() === 'tether')
}

let tethered = false

async function start($: EngineInterface) {
  tethered = tetherLoaded(await $.env.get('CLAUDE_CODE_PLUGIN_DIRS').catch(() => undefined))
  if (tethered) return
  $.clock.every(30_000, () => checkInbox($).catch(quiet))
  $.clock.every(20_000, () => refreshRoster($).catch(quiet))
  $.clock.every(20_000, () => refreshChangeSummary($).catch(quiet))
  void refreshChangeSummary($).catch(quiet)
  void checkInbox($).catch(quiet)
  void refreshRoster($).catch(quiet)
  void postLoadedMods($, (path, init) => fetchJson($, path, init)).catch(quiet)
  for (const spec of COMMANDS) {
    try {
      await $.command.register(spec)
    } catch (err) {
      $.ui.log(`/${spec.name} was refused: ${String(err)}`)
    }
  }
}

export const register: Register = on => {
  on('session.compact', async (_$, e, next) => {
    const result = await next(e)
    if (result.messages && !e.agentId && e.trigger !== 'precompute') {
      heldLessons.clear()
      heldCatalogs.clear()
      heldKeeper.clear()
      refreshLessons = true
    }
    return result
  })

  on('session.start', async ($, e, next) => {
    heldLessons.clear()
    heldCatalogs.clear()
    heldKeeper.clear()
    refreshLessons = true
    await start($)
    return next(e)
  })

  on('prompt.submit', async ($, e, next) => {
    if (tethered) return next(e)
    const lessons = wantsLessons(e) ? await lessonContext($, e.text).catch(() => null) : null
    if (lessons !== null) e = { ...e, context: [...(e.context ?? []), lessons] }
    if (await proxiedByMike($)) return next(e)
    let block: string | null = null
    try {
      block = changesBlock(await takeChangeLines($))
    } catch {
      block = null
    }
    if (block === null) return next(e)
    changesText = undefined
    showStatus($)
    return next({ ...e, context: [...(e.context ?? []), block] })
  })

  on('turn.complete', async ($, e, next) => {
    if (!tethered) void refreshChangeSummary($).catch(quiet)
    return next(e)
  })

  on('command.run', { command: 'seats' }, async ($, e, next) => {
    if (tethered) return next(e)
    await refreshRoster($)
    await $.ui.open({ id: PANE, title: 'mike sessions', focus: true, closeOnEscape: true })
    return {}
  })

  on('command.run', { command: 'recall' }, async ($, e, next) => {
    if (tethered) return next(e)
    const query = e.args.trim()
    if (query === '') return { text: 'Usage: /recall <what to look for>' }
    const body = await fetchJson<{ turns: Turn[]; degraded: boolean }>($, '/api/recall/similar-turns', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ project: await $.session.root(), query, limit: 5 }),
    })
    if (body === null) return { text: 'mike did not answer on ' + INSPECT }
    if (body.turns.length === 0) return { text: `No past turns match "${query}".` }
    const lines = body.turns.map(
      (t, i) =>
        `${i + 1}. [${t.session_id.slice(0, 8)}] asked: ${clip(t.user_prompt ?? '', 160)}\n   answered: ${clip(t.final_assistant_text ?? '', 240)}`,
    )
    const note = body.degraded ? ' (text match only; embeddings unavailable)' : ''
    return { text: `Past turns for "${query}"${note}:\n` + lines.join('\n') }
  })

  on('ui.render', { component: 'Pane', requestId: PANE }, async ($, e) => {
    const { Box, Text, Button } = $.ui.resolve(e)
    const r = await read($, roster)
    const width = Math.max(30, (e.viewport?.columns ?? 80) - 6)
    return (
      <Box flexDirection="column">
        <Box columnGap={2}>
          <Text dimColor>{r.at === '' ? 'loading…' : `${r.seats.length} sessions · ${r.at}`}</Text>
          <Button key="refresh" label="Refresh" hotkey="r" plain onPress={() => refreshRoster($)} />
        </Box>
        {r.error !== null && <Text color="red">{r.error}</Text>}
        {r.seats.map(s => (
          <Box flexDirection="column" marginTop={1}>
            <Text bold={s.isSelf}>
              <Text color={s.liveness === 'live' ? 'green' : 'gray'}>● </Text>
              {s.label}
              {s.role !== null ? ` · ${s.role}` : ''}
              {s.isSelf ? ' (this session)' : ''}
            </Text>
            <Text dimColor>
              {'  '}
              {s.state} · {s.liveness}
            </Text>
            {s.goal !== null && <Text dimColor>{'  ' + clip(s.goal, width)}</Text>}
          </Box>
        ))}
      </Box>
    )
  })
}
