import type { On } from 'claude-code'
import { expect, test } from 'claude-code/testing'

import { counterparts, envelopeSender, isMailOrigin, recipientSession, rosterMatch } from '../hooks/register'

const SELF = '6262dde6-a730-4511-8094-6e79279bb675'
const SENDER = '2408284e-fa87-4d71-baf3-4297423595c3'
const ADDRESS = 'uds:/run/user/1000/cc-socks/42.sock'
const MAIL = `<cross-session-message from="${ADDRESS}" from-session="${SENDER}" from-name="worker-T3-2408284e">\nSchema migration finished\n</cross-session-message>`
const INBOX = { inbox: [{ message_uuid: 'older', from_session_uuid: SENDER }, { message_uuid: 'm-1', from_session_uuid: SENDER }, { message_uuid: 'm-2', from_session_uuid: 'someone-else' }] }

type Asked = { url: string; method?: string; body?: string }

const MODEL = { kind: 'model' } as const

function mikeAnswers(on: On, asked: Asked[], status = 200) {
  on('session.id', async () => ({ value: SELF }))
  on('http.fetch', async ($, e) => {
    asked.push({ url: e.url, method: e.init?.method, body: e.init?.body as string | undefined })
    const text = e.url.endsWith('/messages') ? JSON.stringify(INBOX) : '{"message":{}}'
    return { value: { status: e.url.endsWith('/messages') ? 200 : status, ok: e.url.endsWith('/messages') || status < 400, headers: {}, text } }
  })
}

test('the sender is read from a mike envelope', () => {
  expect(envelopeSender(MAIL)).toEqual({ session: SENDER, handles: [ADDRESS, 'worker-T3-2408284e', SENDER] })
  expect(envelopeSender('plain text from a person')).toBe(null)
  expect(envelopeSender('<cross-session-message from="uds:/x">\nno session\n</cross-session-message>')).toBe(null)
})

test('a recipient is found by address, name or mike: prefix', () => {
  const senders = { [ADDRESS]: SENDER, 'worker-T3-2408284e': SENDER, [SENDER]: SENDER }
  expect(recipientSession(ADDRESS, senders)).toBe(SENDER)
  expect(recipientSession('@worker-T3-2408284e', senders)).toBe(SENDER)
  expect(recipientSession(`mike:${SENDER}`, senders)).toBe(SENDER)
  expect(recipientSession('api-worker', senders)).toBe(null)
})

test('a reply to a mike sender goes through mike as a reply to its newest message', async ($, on) => {
  const asked: Asked[] = []
  const toasts: string[] = []
  on('ui.toast', async (_$, e) => {
    toasts.push(e.text)
    return { value: undefined }
  })
  mikeAnswers(on, asked)
  let reachedNative = false
  on('session.receive', async ($, e) => ({ text: e.text }))
  on('session.send', async () => {
    reachedNative = true
    return { isDelivered: true }
  })
  await $.session.receive({ origin: { kind: 'peer' }, text: MAIL })
  const sent = await $.session.send({ to: ADDRESS, text: 'Thanks, rebasing now', origin: MODEL })
  expect(sent.isDelivered).toBe(true)
  expect(reachedNative).toBe(false)
  expect(toasts).toEqual([`✉ Reply to ${ADDRESS} sent through mike's mailbox`])
  const post = asked.find(a => a.method === 'POST' && a.url.endsWith('/message'))
  expect(post?.url).toBe(`http://127.0.0.1:9989/api/sessions/${SENDER}/message`)
  expect(JSON.parse(post?.body ?? '{}')).toEqual({ message: 'Thanks, rebasing now', from_session_id: SELF, in_reply_to: 'm-1' })
})

test('a send to a session that never wrote through mike goes out natively', async ($, on) => {
  const asked: Asked[] = []
  mikeAnswers(on, asked)
  let reachedNative = false
  on('session.send', async () => {
    reachedNative = true
    return { isDelivered: true }
  })
  await $.session.send({ to: 'api-worker', text: 'hello', origin: MODEL })
  expect(reachedNative).toBe(true)
  expect(asked.some(a => a.method === 'POST')).toBe(false)
})

test('when mike refuses, a reply to a socket address falls back to native delivery', async ($, on) => {
  const asked: Asked[] = []
  mikeAnswers(on, asked, 500)
  on('ui.log', async () => ({ value: undefined }))
  on('session.receive', async ($, e) => ({ text: e.text }))
  let reachedNative = false
  on('session.send', async () => {
    reachedNative = true
    return { isDelivered: true }
  })
  await $.session.receive({ origin: { kind: 'peer' }, text: MAIL })
  const sent = await $.session.send({ to: ADDRESS, text: 'hello', origin: MODEL })
  expect(sent.isDelivered).toBe(true)
  expect(reachedNative).toBe(true)
})

test('when mike refuses a reply by name, the send fails with mike\'s reason', async ($, on) => {
  const asked: Asked[] = []
  mikeAnswers(on, asked, 500)
  on('session.receive', async ($, e) => ({ text: e.text }))
  await $.session.receive({ origin: { kind: 'peer' }, text: MAIL })
  const sent = await $.session.send({ to: 'worker-T3-2408284e', text: 'hello', origin: MODEL })
  expect(sent.isDelivered).toBe(false)
  expect(sent.reason).toContain('mike refused the message (500)')
})

const CHANNEL_MAIL = `<channel source="mike" from_name="coordinator-d58ac250" from_role="coordinator" from_session="${SENDER}" kind="mailbox_message" message_id="x">\nPlease rebase\n</channel>`

test('a plain session reads the sender from a mike channel message too', () => {
  expect(envelopeSender(CHANNEL_MAIL)).toEqual({ session: SENDER, handles: ['coordinator-d58ac250', SENDER] })
  expect(envelopeSender('<channel source="slack" from_session="' + SENDER + '">x</channel>')).toBe(null)
  expect(isMailOrigin({ kind: 'channel', server: 'mike' })).toBe(true)
  expect(isMailOrigin({ kind: 'channel', server: 'slack' })).toBe(false)
  expect(isMailOrigin({ kind: 'user' })).toBe(false)
})

test('a plain session answers a mike channel message through mike, by name or session', async ($, on) => {
  const asked: Asked[] = []
  mikeAnswers(on, asked)
  let reachedNative = false
  on('session.receive', async ($, e) => ({ text: e.text }))
  on('session.send', async () => {
    reachedNative = true
    return { isDelivered: true }
  })
  await $.session.receive({ origin: { kind: 'channel', server: 'mike' }, text: CHANNEL_MAIL })
  const sent = await $.session.send({ to: 'coordinator-d58ac250', text: 'Rebased', origin: MODEL })
  expect(sent.isDelivered).toBe(true)
  expect(reachedNative).toBe(false)
  const post = asked.find(a => a.method === 'POST' && a.url.endsWith('/message'))
  expect(post?.url).toBe(`http://127.0.0.1:9989/api/sessions/${SENDER}/message`)
  expect(JSON.parse(post?.body ?? '{}')).toEqual({ message: 'Rebased', from_session_id: SELF, in_reply_to: 'm-1' })
})

test('a roster name resolves to a session other than this one', () => {
  const roster = [{ session_id: SENDER, name: 'c4-s' }, { session_id: SELF, name: 'c4-r' }]
  expect(rosterMatch('c4-s', SELF, roster)).toBe(SENDER)
  expect(rosterMatch('@c4-s', SELF, roster)).toBe(SENDER)
  expect(rosterMatch(SENDER, SELF, roster)).toBe(SENDER)
  expect(rosterMatch('c4-r', SELF, roster)).toBe(null)
  expect(rosterMatch('uds:/x.sock', SELF, roster)).toBe(null)
  expect(rosterMatch('nobody', SELF, roster)).toBe(null)
})

test('a reply to the from_name of mail that was never parsed still goes through mike, found by the roster', async ($, on) => {
  const asked: Asked[] = []
  on('session.id', async () => ({ value: SELF }))
  on('session.root', async () => ({ value: '/repo' }))
  on('http.fetch', async (_$, e) => {
    asked.push({ url: e.url, method: e.init?.method, body: e.init?.body as string | undefined })
    const text = e.url.endsWith('/roster')
      ? JSON.stringify({ roster: [{ session_id: SENDER, name: 'c4-s' }] })
      : e.url.endsWith('/messages')
        ? JSON.stringify({ inbox: [{ message_uuid: 'unthreaded-1', from_session_uuid: SENDER }] })
        : '{"message":{}}'
    return { value: { status: 200, ok: true, headers: {}, text } }
  })
  let reachedNative = false
  on('session.send', async () => {
    reachedNative = true
    return { isDelivered: true }
  })
  const sent = await $.session.send({ to: 'c4-s', text: 'Got it', origin: MODEL })
  expect(sent.isDelivered).toBe(true)
  expect(reachedNative).toBe(false)
  const post = asked.find(a => a.method === 'POST' && a.url.endsWith('/message'))
  expect(post?.url).toBe(`http://127.0.0.1:9989/api/sessions/${SENDER}/message`)
  expect(JSON.parse(post?.body ?? '{}')).toEqual({ message: 'Got it', from_session_id: SELF, in_reply_to: 'unthreaded-1' })
})

test('a session id or its eight-character prefix is a recipient even for a session with no name', () => {
  const roster = [{ session_id: SENDER, name: null }]
  expect(rosterMatch(SENDER, SELF, [])).toBe(SENDER)
  expect(rosterMatch(SELF, SELF, [])).toBe(null)
  expect(rosterMatch(SENDER.slice(0, 8), SELF, roster)).toBe(SENDER)
  expect(rosterMatch('deadbeef', SELF, roster)).toBe(null)
})

test('among sessions sharing a name the newest registered one is chosen', () => {
  const roster = [
    { session_id: 'old', name: 'c4-s', registered_at: '2026-10-08T07:00:00Z' },
    { session_id: SENDER, name: 'c4-s', registered_at: '2026-10-08T07:30:00Z' },
  ]
  expect(rosterMatch('c4-s', SELF, roster)).toBe(SENDER)
})

test('a reply by name works when the sender is in another project: the name comes from mike for the session in this inbox', async ($, on) => {
  const asked: Asked[] = []
  on('session.id', async () => ({ value: SELF }))
  on('session.root', async () => ({ value: '/repo-a' }))
  on('http.fetch', async (_$, e) => {
    asked.push({ url: e.url, method: e.init?.method, body: e.init?.body as string | undefined })
    const text = e.url.endsWith('/roster')
      ? JSON.stringify({ roster: [{ session_id: SELF, name: 'c4-r3' }] })
      : e.url.endsWith(`${SENDER}/goal`)
        ? JSON.stringify({ name: 'c4-s3', role: null })
        : e.url.endsWith('/messages')
          ? JSON.stringify({ inbox: [{ message_uuid: 'mail-1', from_session_uuid: SENDER, queued_at: '2026-10-08T07:49:00Z' }], outbox: [] })
          : '{"message":{}}'
    return { value: { status: 200, ok: true, headers: {}, text } }
  })
  let reachedNative = false
  on('session.send', async () => {
    reachedNative = true
    return { isDelivered: true }
  })
  const sent = await $.session.send({ to: 'c4-s3', text: 'on it', origin: MODEL })
  expect(sent.isDelivered).toBe(true)
  expect(reachedNative).toBe(false)
  const post = asked.find(a => a.method === 'POST' && a.url.endsWith('/message'))
  expect(post?.url).toBe(`http://127.0.0.1:9989/api/sessions/${SENDER}/message`)
  expect(JSON.parse(post?.body ?? '{}')).toEqual({ message: 'on it', from_session_id: SELF, in_reply_to: 'mail-1' })
})

test('counterparts are the other sessions in the inbox and outbox, newest first, never this one', () => {
  const list = counterparts(
    SELF,
    [{ from_session_uuid: 'a', to_session_uuid: SELF, queued_at: '2026-10-08T07:00:00Z' }, { from_session_uuid: SELF, to_session_uuid: SELF }],
    [{ from_session_uuid: SELF, to_session_uuid: 'b', queued_at: '2026-10-08T08:00:00Z' }],
  )
  expect(list.map(c => c.id)).toEqual(['b', 'a'])
})

function linkAnswers(on: On, asked: Asked[], link: string) {
  on('session.id', async () => ({ value: SELF }))
  on('http.fetch', async ($, e) => {
    asked.push({ url: e.url, method: e.init?.method, body: e.init?.body as string | undefined })
    const text = e.url.endsWith('/messages') ? JSON.stringify(INBOX) : e.url.endsWith('/reply-link') ? link : '{"message":{}}'
    return { value: { status: 200, ok: true, headers: {}, text } }
  })
}

test('when Jev links the reply to an older message, mike gets that message as in_reply_to', async ($, on) => {
  const asked: Asked[] = []
  linkAnswers(on, asked, JSON.stringify({ in_reply_to: 'older', decided_by: 'jev', probability: 0.9 }))
  on('session.receive', async ($, e) => ({ text: e.text }))
  await $.session.receive({ origin: { kind: 'peer' }, text: MAIL })
  const sent = await $.session.send({ to: ADDRESS, text: 'About the schema question', origin: MODEL })
  expect(sent.isDelivered).toBe(true)
  const link = asked.find(a => a.url.endsWith('/reply-link'))
  expect(link?.url).toBe(`http://127.0.0.1:9989/api/sessions/${SELF}/mail/reply-link`)
  expect(JSON.parse(link?.body ?? '{}')).toEqual({ sender: SENDER, reply: 'About the schema question', candidates: ['older', 'm-1'] })
  const post = asked.find(a => a.method === 'POST' && a.url.endsWith('/message'))
  expect(JSON.parse(post?.body ?? '{}')).toEqual({ message: 'About the schema question', from_session_id: SELF, in_reply_to: 'older' })
})

test('when Jev says the reply answers none of them, the message goes without in_reply_to', async ($, on) => {
  const asked: Asked[] = []
  linkAnswers(on, asked, JSON.stringify({ in_reply_to: null, decided_by: 'jev', probability: 0.8 }))
  on('session.receive', async ($, e) => ({ text: e.text }))
  await $.session.receive({ origin: { kind: 'peer' }, text: MAIL })
  await $.session.send({ to: ADDRESS, text: 'New subject', origin: MODEL })
  const post = asked.find(a => a.method === 'POST' && a.url.endsWith('/message'))
  expect(JSON.parse(post?.body ?? '{}')).toEqual({ message: 'New subject', from_session_id: SELF })
})

test('when the decisions layer is off, the reply links to the newest message as before', async ($, on) => {
  const asked: Asked[] = []
  linkAnswers(on, asked, JSON.stringify({ in_reply_to: null, decided_by: 'off', probability: null }))
  on('session.receive', async ($, e) => ({ text: e.text }))
  await $.session.receive({ origin: { kind: 'peer' }, text: MAIL })
  await $.session.send({ to: ADDRESS, text: 'ok', origin: MODEL })
  const post = asked.find(a => a.method === 'POST' && a.url.endsWith('/message'))
  expect(JSON.parse(post?.body ?? '{}')).toEqual({ message: 'ok', from_session_id: SELF, in_reply_to: 'm-1' })
})
