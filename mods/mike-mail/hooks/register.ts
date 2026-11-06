import type { EngineInterface, Register, SessionSendInput } from 'claude-code'

import type { Senders } from '../types'

const INSPECT = 'http://127.0.0.1:9989'
const SENDERS = { plugin: 'mike-mail', key: 'senders' } as const
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i
const ENVELOPE = /<cross-session-message\b([^>]*)>/

type InboxRow = { message_uuid: string; from_session_uuid: string | null; queued_at?: string }

const CHANNEL = /<channel\b([^>]*\bsource="mike"[^>]*)>/

export function envelopeSender(text: string): { session: string; handles: string[] } | null {
  const channel = CHANNEL.exec(text)?.[1]
  const attributes = channel ?? ENVELOPE.exec(text)?.[1]
  if (attributes === undefined) return null
  const read = (name: string) => new RegExp(`\\b${name}="([^"]+)"`).exec(attributes)?.[1]
  const dashed = channel === undefined
  const session = read(dashed ? 'from-session' : 'from_session')
  if (session === undefined || !UUID.test(session)) return null
  const handles = [dashed ? read('from') : undefined, read(dashed ? 'from-name' : 'from_name'), session].filter(
    (h): h is string => h !== undefined,
  )
  return { session, handles }
}

export function isMailOrigin(origin: { kind: string; server?: string }): boolean {
  return origin.kind === 'peer' || (origin.kind === 'channel' && origin.server === 'mike')
}

export function recipientSession(to: string, senders: Senders): string | null {
  const bare = to.trim().replace(/^@/, '').replace(/^mike:/, '')
  if (senders[bare] !== undefined) return senders[bare]
  return null
}

type RosterRow = { session_id: string; name: string | null; registered_at?: string }

export function rosterMatch(to: string, self: string, roster: RosterRow[]): string | null {
  const bare = to.trim().replace(/^@/, '').replace(/^mike:/, '')
  if (bare === '' || bare.startsWith('uds:')) return null
  if (UUID.test(bare)) return bare === self ? null : bare
  const prefix = /^[0-9a-f]{8,}$/i.test(bare)
  const hits = roster.filter(
    r => r.session_id !== self && (r.name === bare || (prefix && r.session_id.startsWith(bare.toLowerCase()))),
  )
  if (hits.length === 0) return null
  const newest = [...hits].sort((a, b) => (b.registered_at ?? '').localeCompare(a.registered_at ?? ''))[0]
  return newest?.session_id ?? null
}

async function rosterRows($: EngineInterface, root: string, inactive: boolean): Promise<RosterRow[]> {
  const path = `${INSPECT}/api/workspace/projects/${encodeURIComponent(root)}/roster`
  const res = await $.http.fetch(inactive ? `${path}?include_inactive=true` : path)
  if (!res.ok) return []
  return (JSON.parse(res.text) as { roster?: RosterRow[] }).roster ?? []
}

const NAMES = new Map<string, string>()
const MAX_COUNTERPARTS = 40

type MailRow = { from_session_uuid: string | null; to_session_uuid: string | null; queued_at?: string }

export function counterparts(self: string, inbox: MailRow[], outbox: MailRow[]): { id: string; at: string }[] {
  const seen = new Map<string, string>()
  const note = (id: string | null, at: string | undefined) => {
    if (id === null || id === self) return
    const old = seen.get(id)
    if (old === undefined || (at ?? '') > old) seen.set(id, at ?? '')
  }
  for (const m of inbox) note(m.from_session_uuid, m.queued_at)
  for (const m of outbox) note(m.to_session_uuid, m.queued_at)
  return [...seen.entries()]
    .map(([id, at]) => ({ id, at }))
    .sort((a, b) => b.at.localeCompare(a.at))
    .slice(0, MAX_COUNTERPARTS)
}

async function nameOfSession($: EngineInterface, id: string): Promise<string | null> {
  const known = NAMES.get(id)
  if (known !== undefined) return known
  try {
    const res = await $.http.fetch(`${INSPECT}/api/sessions/${id}/goal`)
    if (!res.ok) return null
    const name = (JSON.parse(res.text) as { name?: string | null }).name
    if (typeof name !== 'string' || name === '') return null
    NAMES.set(id, name)
    return name
  } catch {
    return null
  }
}

async function counterpartRows($: EngineInterface, self: string): Promise<RosterRow[]> {
  try {
    const res = await $.http.fetch(`${INSPECT}/api/sessions/${self}/messages`)
    if (!res.ok) return []
    const box = JSON.parse(res.text) as { inbox?: MailRow[]; outbox?: MailRow[] }
    const rows: RosterRow[] = []
    for (const c of counterparts(self, box.inbox ?? [], box.outbox ?? [])) {
      rows.push({ session_id: c.id, name: await nameOfSession($, c.id), registered_at: c.at })
    }
    return rows
  } catch {
    return []
  }
}

async function rosterSession($: EngineInterface, to: string): Promise<string | null> {
  try {
    const self = await $.session.id()
    const fromMail = rosterMatch(to, self, await counterpartRows($, self))
    if (fromMail !== null) return fromMail
    const root = await $.session.root()
    return (
      rosterMatch(to, self, await rosterRows($, root, false)) ??
      rosterMatch(to, self, await rosterRows($, root, true))
    )
  } catch {
    return null
  }
}

async function remember($: EngineInterface, session: string, handles: string[]) {
  const { value = {} } = await $.state.get(SENDERS)
  const next: Senders = { ...value }
  for (const handle of handles) next[handle] = session
  await $.state.set(SENDERS, next)
}

async function newestFrom($: EngineInterface, self: string, sender: string): Promise<string | undefined> {
  try {
    const res = await $.http.fetch(`${INSPECT}/api/sessions/${self}/messages`)
    if (!res.ok) return undefined
    const inbox = (JSON.parse(res.text) as { inbox: InboxRow[] }).inbox
    return inbox.filter(m => m.from_session_uuid === sender).at(-1)?.message_uuid
  } catch {
    return undefined
  }
}

async function sendThroughMike($: EngineInterface, e: SessionSendInput, to: string): Promise<string | null> {
  const self = await $.session.id()
  const inReplyTo = await newestFrom($, self, to)
  const body: Record<string, string> = { message: e.text, from_session_id: self }
  if (inReplyTo !== undefined) body.in_reply_to = inReplyTo
  try {
    const res = await $.http.fetch(`${INSPECT}/api/sessions/${to}/message`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body),
    })
    return res.ok ? null : `mike refused the message (${res.status}): ${res.text}`
  } catch (err) {
    return `mike did not answer on ${INSPECT}: ${String(err)}`
  }
}

export const register: Register = on => {
  on('session.receive', async ($, e, next) => {
    const sender = isMailOrigin(e.origin) ? envelopeSender(e.text) : null
    $.ui.log(`mike-mail: received origin=${JSON.stringify(e.origin)} sender=${sender?.session ?? 'none'} text=${JSON.stringify(e.text.slice(0, 160))}`)
    if (sender !== null) await remember($, sender.session, sender.handles)
    return next(e)
  })

  on('session.send', async ($, e, next) => {
    const { value = {} } = await $.state.get(SENDERS)
    const to = recipientSession(e.to, value) ?? (await rosterSession($, e.to))
    $.ui.log(`mike-mail: send to=${JSON.stringify(e.to)} known=${Object.keys(value).length} resolved=${to ?? 'none'}`)
    if (to === null) return next(e)
    const refused = await sendThroughMike($, e, to)
    const who = e.to.replace(/^(mike:|@)/, '')
    if (refused === null) {
      $.ui.toast(`✉ Reply to ${who} sent through mike's mailbox`)
      return { isDelivered: true }
    }
    if (e.to.startsWith('uds:')) {
      $.ui.log(`mike-mail: ${refused}; sending natively instead`)
      $.ui.toast(`✉ mike did not take the reply to ${who}; sent it the usual way`)
      return next(e)
    }
    $.ui.toast(`✉ Reply to ${who} not sent: ${refused}`)
    return { isDelivered: false, reason: refused }
  })
}
