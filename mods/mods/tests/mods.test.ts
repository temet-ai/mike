import type { On } from 'claude-code'
import type { Engine } from 'claude-code/testing'
import { expect, test } from 'claude-code/testing'

import { HIDE_COMMAND, chipsOf, columnsOf, commandName, shownButtons, withHidden, describe, entryFrom, footerText, groupsOf, headline, labelsOf, matches, pluginDirs, pressedCommand, rowOf, shortPath, sourceOf, statusWords, takeStatus, totals } from '../hooks/register'
import { LOOK, chipAt, lookOf } from '../hooks/row'

const MANIFEST = JSON.stringify({
  name: 'fleet-comms',
  version: '0.2.0',
  description: 'Shows the messages of the fleet this session works in, read from mike.',
})
const SELF = JSON.stringify({
  tagline: "The fleet's messages above the prompt",
  offers: {
    commands: ['/fleet-comms'],
    panes: ['Fleet'],
    bands: ['latest message'],
    status: ['NEEDS YOU'],
    acts: ['marks you as read'],
    buttons: [{ label: 'Fleet', command: '/fleet-comms' }, { label: 'no slash', command: 'fleet' }, { command: '/unnamed' }],
  },
})

const HINT_PROPS = { isDraft: false, isWorking: false, hint: '? for shortcuts' }

function mountHint($: Engine, surface: 'terminal' | 'desktop' = 'terminal', props = HINT_PROPS, requestId = 'hint') {
  return $.ui.mount({ plugin: 'mods', surface, component: 'PromptHint', props, requestId })
}

const PANE_PROPS = {
  title: 'Mods',
  isFocused: true,
  bodyColumns: 110,
  placement: 'dock' as const,
  scroll: { offset: 0, bodyRows: 60 },
  view: {},
}

function twoMods(on: On) {
  on('env.get', async (_$, e) =>
    e.name === 'CLAUDE_CODE_PLUGIN_DIRS' ? { value: '/m/mods:/m/mail' } : { value: undefined },
  )
  on('fs.read', async (_$, e, next) => {
    const files: Record<string, string> = {
      '/m/mods/.claude-plugin/plugin.json': JSON.stringify({ name: 'mods', version: '0.2.0' }),
      '/m/mods/.claude-plugin/mod.json': JSON.stringify({
        tagline: 'Lists mods',
        offers: { commands: ['/mods'], panes: ['Mods'], buttons: [{ label: 'Mods', command: '/mods' }] },
      }),
      '/m/mail/.claude-plugin/plugin.json': JSON.stringify({ name: 'mike-mail', version: '0.1.0' }),
      '/m/mail/.claude-plugin/mod.json': JSON.stringify({
        tagline: 'Replies go through mike',
        offers: { commands: ['/inbox'], acts: ['routes replies via mike'], buttons: [{ label: 'Inbox', command: '/inbox' }] },
      }),
    }
    return e.path in files ? { value: files[e.path] } : next(e)
  })
  on('ui.open', async () => ({ value: { isPlaced: true } }))
}

test('a mod sorts what it adds into what you type, what you see and what it does itself', () => {
  const entry = entryFrom('/home/u/.claude/mods/fleet-comms', MANIFEST, '/repo/mods/fleet-comms', SELF)
  expect(entry.tagline).toBe("The fleet's messages above the prompt")
  expect(sourceOf(entry)).toBe('repo')
  expect(columnsOf(entry)).toEqual({
    type: ['/fleet-comms'],
    see: [
      { where: 'pane', what: 'Fleet' },
      { where: 'band', what: 'latest message' },
      { where: 'status', what: 'NEEDS YOU' },
    ],
    acts: ['marks you as read'],
  })
  const sum = totals([entry, entryFrom('/x/plain', undefined)])
  expect([sum.type.length, sum.see.length, sum.acts.length]).toEqual([1, 3, 1])
})

test('a mod asks for a button above the prompt with a label and a /command; anything else is dropped', () => {
  const entry = entryFrom('/m/fleet-comms', MANIFEST, undefined, SELF)
  expect(entry.offers?.buttons).toEqual([{ label: 'Fleet', command: '/fleet-comms' }])
  const plain = entryFrom('/x/plain', undefined, undefined, JSON.stringify({ offers: { commands: ['/x'] } }))
  expect(plain.offers?.buttons).toBeUndefined()
  expect(rowOf({ entries: [plain, entry, entryFrom('/x/none', undefined)], others: [] })).toEqual([
    { label: 'Fleet', command: '/fleet-comms' },
  ])
})

test('the text form reads in plain words', () => {
  const entry = entryFrom('/home/u/.claude/mods/fleet-comms', MANIFEST, '/repo/mods/fleet-comms', SELF)
  const text = describe({ entries: [entry], others: ['zeta', 'alpha'] })
  expect(text).toContain('3 mods loaded in this session')
  expect(text).toContain('fleet-comms 0.2.0  (repo/mods/fleet-comms)')
  expect(text).toContain('/command  /fleet-comms')
  expect(text).toContain('produces  Fleet (pane), latest message (band), NEEDS YOU (status)')
  expect(text).toContain('auto      marks you as read')
  expect(text).toContain('+ 2 more from a marketplace or --plugin-dir: alpha, zeta')
})

test('a mod with no manifest or a broken one still shows, by folder name', () => {
  const missing = entryFrom('/x/plain-mod', undefined)
  const broken = entryFrom('/x/broken/', '{not json')
  expect(missing.name).toBe('plain-mod')
  expect(broken.name).toBe('broken')
  expect(sourceOf(missing)).toBe('folder')
  expect(columnsOf(missing)).toEqual({ type: [], see: [], acts: [] })
  expect(describe({ entries: [missing], others: [] })).toContain('No description')
})

test('the filter matches the name, the tagline and anything a mod adds', () => {
  const entry = entryFrom('/m/fleet-comms', MANIFEST, undefined, SELF)
  expect(matches(entry, '')).toBe(true)
  expect(matches(entry, 'FLEET')).toBe(true)
  expect(matches(entry, 'needs you')).toBe(true)
  expect(matches(entry, 'telemetry')).toBe(false)
})

test('mods group as this project first, then each family, then the global rest', () => {
  const family = (name: string) => entryFrom(`/g/${name}`, JSON.stringify({ name }), undefined, JSON.stringify({ family: 'mike' }))
  const loose = entryFrom('/g/mods', JSON.stringify({ name: 'mods' }))
  const local = entryFrom('/p/app/.claude/skills/deploy', JSON.stringify({ name: 'deploy' }), undefined, JSON.stringify({ family: 'mike' }), 'project')
  const groups = groupsOf({ entries: [loose, family('mike-mail'), local, family('fleet-comms')], others: [], project: 'app' })
  expect(groups.map(g => [g.title, g.entries.map(e => e.name)])).toEqual([
    ['This project · app', ['deploy']],
    ['mike', ['fleet-comms', 'mike-mail']],
    ['Global', ['mods']],
  ])
})

test('a mod in the project\'s .claude/skills shows under This project', async ($, on) => {
  on('env.get', async (_$, e) =>
    e.name === 'CLAUDE_CODE_PLUGIN_DIRS' ? { value: '/m/mods' } : e.name === 'HOME' ? { value: '/home/u' } : { value: undefined },
  )
  on('session.root', async () => ({ value: '/p/app' }))
  on('fs.list', async (_$, e) =>
    e.path === '/p/app/.claude/skills'
      ? { value: [{ name: 'deploy-helper', kind: 'dir', size: 0, mtimeMs: 0, isLink: false }] }
      : { value: [] },
  )
  on('fs.exists', async (_$, e) => ({ value: e.path === '/p/app/.claude/skills/deploy-helper/.claude-plugin/plugin.json' }))
  on('fs.read', async (_$, e, next) => {
    const files: Record<string, string> = {
      '/m/mods/.claude-plugin/plugin.json': JSON.stringify({ name: 'mods', version: '0.2.0' }),
      '/p/app/.claude/skills/deploy-helper/.claude-plugin/plugin.json': JSON.stringify({ name: 'deploy-helper', version: '1.0.0' }),
      '/p/app/.claude/skills/deploy-helper/.claude-plugin/mod.json': JSON.stringify({ tagline: 'Ships this app', offers: { commands: ['/ship'] } }),
    }
    return e.path in files ? { value: files[e.path] } : next(e)
  })
  const ran = await $.command.run({ command: 'mods', args: 'text' })
  expect(ran.text).toContain('== This project · app (loads only here)')
  expect(ran.text).toContain('deploy-helper 1.0.0')
  expect(ran.text).toContain('== Global (every project)')
  expect((ran.text ?? '').indexOf('deploy-helper')).toBeLessThan((ran.text ?? '').indexOf('== Global'))
})

test('no mods says so', () => {
  expect(headline({ entries: [], others: [] })).toBe('No mods are loaded in this session.')
})

test('paths under home are shortened and plugin dirs are deduplicated', () => {
  expect(shortPath('/home/u/.claude/mods/x', '/home/u')).toBe('~/.claude/mods/x')
  expect(shortPath('/mnt/repo/mods/x', '/home/u')).toBe('/mnt/repo/mods/x')
  expect(pluginDirs('/a/mike:/a/mods/:/a/mike::')).toEqual(['/a/mike', '/a/mods'])
  expect(pluginDirs(undefined)).toEqual([])
})

test('/mods text answers with the folders on CLAUDE_CODE_PLUGIN_DIRS', async ($, on) => {
  twoMods(on)
  const ran = await $.command.run({ command: 'mods', args: 'text' })
  expect(ran.text).toContain('mods 0.2.0')
  expect(ran.text).toContain('/command  /mods')
  expect(ran.text).toContain('auto      routes replies via mike')
})

test('/mods draws name, /command, produces and auto, with every tagline in its row', async ($, on) => {
  twoMods(on)
  await $.command.run({ command: 'mods', args: '' })
  const pane = await $.ui.mount({ plugin: 'mods', surface: 'terminal', component: 'Pane', props: PANE_PROPS, requestId: 'mods' })
  expect(await pane.find({ text: /^name$/ })).toBeDefined()
  expect(await pane.find({ text: /^\/command$/ })).toBeDefined()
  expect(await pane.find({ text: /^produces$/ })).toBeDefined()
  expect(await pane.find({ text: /^auto$/ })).toBeDefined()
  expect(await pane.find({ text: /commands you can type/ })).toBeUndefined()
  expect(await pane.find({ key: 'run-/mods' })).toBeDefined()
  expect(await pane.find({ text: /routes replies via mike/ })).toBeDefined()
  expect(await pane.find({ text: /^Lists mods$/ })).toBeDefined()
  expect(await pane.find({ text: /^Replies go through mike$/ })).toBeDefined()
})

test('clicking a /command closes the pane and runs it', async ($, on) => {
  twoMods(on)
  const ran: string[] = []
  const closed: string[] = []
  on('command.run', async (_$, e, next) => {
    if (e.command !== 'inbox') return next(e)
    ran.push(e.command)
    return { text: 'inbox opened' }
  })
  on('ui.close', async (_$, e) => {
    closed.push(e.id)
    return { value: undefined }
  })
  await $.command.run({ command: 'mods', args: '' })
  const pane = await $.ui.mount({ plugin: 'mods', surface: 'terminal', component: 'Pane', props: PANE_PROPS, requestId: 'mods' })
  await pane.press({ key: 'run-/inbox' })
  expect(closed).toEqual(['mods'])
  expect(ran).toEqual(['inbox'])
})

test("a mod's status line joins the footer after the chips, named by its chip, without repeating the chip's word", () => {
  const changes = entryFrom('/m/what-changed', JSON.stringify({ name: 'what-changed' }), undefined, JSON.stringify({ offers: { buttons: [{ label: 'Changes', command: '/changes' }] } }))
  const mike = entryFrom('/m/mike', JSON.stringify({ name: 'mike' }), undefined, JSON.stringify({ offers: { buttons: [{ label: 'Sessions', command: '/seats' }] } }))
  const guard = entryFrom('/m/java-guard', JSON.stringify({ name: 'java-guard' }), undefined, JSON.stringify({ offers: { status: ['java guard refused N'] } }))
  const labels = labelsOf({ entries: [mike, changes, guard], others: [] })
  expect(labels).toEqual({ mike: 'Sessions', 'what-changed': 'Changes' })
  const words = statusWords(
    { 'what-changed': 'changes: nothing of yours uncommitted', mike: '3 changes by others', 'java-guard': 'java guard refused 2', ghost: '  ' },
    labels,
  )
  expect(words).toEqual(['Sessions: 3 changes by others', 'Changes: nothing of yours uncommitted'])
  expect(statusWords({}, labels)).toEqual([])
  const chips = chipsOf([{ label: 'Mods', command: '/mods' }, { label: 'Inbox', command: '/inbox' }])
  const whole = 'Sessions: 3 changes by others · Changes: nothing of yours uncommitted'
  expect(footerText(words, '? for shortcuts', undefined, chips)).toEqual({ statuses: whole, hint: '? for shortcuts' })
  expect(footerText([], '? for shortcuts', undefined, chips)).toEqual({ statuses: '', hint: '? for shortcuts' })
  expect(footerText(words, '? for shortcuts', 110, chips)).toEqual({ statuses: whole, hint: '? for shortcuts' })
  expect(footerText(words, '? for shortcuts', 80, chips)).toEqual({
    statuses: 'Sessions: 3 changes by others · Changes: nothin…',
    hint: '? for shortcuts',
  })
  expect(footerText(words, '? for shortcuts', 60, chips)).toEqual({ statuses: 'Sessions: 3 changes by othe…', hint: '? for …' })
  expect(footerText(words, '? for shortcuts', 40, chips)).toEqual({ statuses: 'Session…', hint: '' })
})

test('chips are hit by column: each is its label plus a space each side, one space apart', () => {
  const chips = chipsOf([{ label: 'Mods', command: '/mods' }, { label: 'Inbox', command: '/inbox' }])
  expect(chips.map(c => c.label)).toEqual(['Mods', 'Inbox', 'Hide'])
  expect(chips[2]).toMatchObject({ command: HIDE_COMMAND, dim: true })
  expect(chipAt(chips, 0, 0)).toBe(0)
  expect(chipAt(chips, 5, 0)).toBe(0)
  expect(chipAt(chips, 6, 0)).toBeNull()
  expect(chipAt(chips, 7, 0)).toBe(1)
  expect(chipAt(chips, 13, 0)).toBe(1)
  expect(chipAt(chips, 15, 0)).toBe(2)
  expect(chipAt(chips, 21, 0)).toBeNull()
  expect(chipAt(chips, 3, 1)).toBeNull()
  expect(chipAt(chips, -1, 0)).toBeNull()
  const rest = { hover: null, down: null }
  expect(lookOf(chips[0]!, 0, rest)).toEqual(LOOK.rest)
  expect(lookOf(chips[2]!, 2, rest)).toEqual(LOOK.dim)
  expect(lookOf(chips[0]!, 0, { hover: 0, down: null })).toEqual(LOOK.hover)
  expect(lookOf(chips[0]!, 0, { hover: 0, down: 0 })).toEqual(LOOK.down)
  expect(lookOf({ label: 'Fleet', command: '/fleet-comms', alert: true }, 0, rest)).toEqual(LOOK.alert)
  expect(pressedCommand({ press: '/inbox' }, chips)).toBe('/inbox')
  expect(pressedCommand({ press: '/rm -rf' }, chips)).toBeNull()
  expect(pressedCommand('nope', chips)).toBeNull()
})

test('the hint line under the prompt carries the chips, then the engine\'s own hint: hover lights one, a press on it runs the command', async ($, on) => {
  twoMods(on)
  const ran: { command: string; args: string }[] = []
  on('command.run', async (_$, e, next) => {
    if (e.command !== 'inbox') return next(e)
    ran.push({ command: e.command, args: e.args })
    return { text: 'inbox opened' }
  })
  const toasts: string[] = []
  on('ui.toast', async (_$, e) => {
    toasts.push(e.text)
    return { value: undefined }
  })
  const band = await mountHint($)
  expect(await band.find({ type: 'Client', key: 'row' })).toMatchObject({ props: { module: 'hooks/row.tsx' } })
  const hints = await band.findAll({ type: 'Text', text: '? for shortcuts' })
  expect(hints.some(t => t.props.dimColor === true)).toBe(true)
  const working = await mountHint($, 'terminal', { isDraft: false, isWorking: true, hint: 'esc to interrupt' }, 'hint-working')
  expect(await working.find({ type: 'Text', text: 'esc to interrupt' })).toBeDefined()
  expect(await working.find({ type: 'Client', key: 'row' })).toBeDefined()
  await working.unmount()
  const chip = async (label: string) => (await band.find({ type: 'Text', text: ` ${label} `, in: 'row' }))?.props
  expect(await chip('Mods')).toMatchObject({ backgroundColor: LOOK.rest.bg, bold: true })
  expect(await chip('Inbox')).toMatchObject({ backgroundColor: LOOK.rest.bg })
  expect(await chip('Hide')).toMatchObject({ backgroundColor: LOOK.dim.bg, bold: false })
  await band.pointer({ type: 'enter', x: 8, y: 0, in: 'row' })
  await band.pointer({ type: 'move', x: 8, y: 0, in: 'row' })
  expect(await chip('Inbox')).toMatchObject({ backgroundColor: LOOK.hover.bg })
  expect(await chip('Mods')).toMatchObject({ backgroundColor: LOOK.rest.bg })
  await band.pointer({ type: 'down', x: 8, y: 0, button: 'left', in: 'row' })
  expect(await chip('Inbox')).toMatchObject({ backgroundColor: LOOK.down.bg })
  await band.pointer({ type: 'up', x: 9, y: 0, button: 'left', in: 'row' })
  expect(ran).toEqual([{ command: 'inbox', args: '' }])
  expect(toasts).toEqual(['inbox opened'])
  expect(await chip('Inbox')).toMatchObject({ backgroundColor: LOOK.hover.bg })
  await band.pointer({ type: 'leave', x: 9, y: 0, in: 'row' })
  expect(await chip('Inbox')).toMatchObject({ backgroundColor: LOOK.rest.bg })
  await band.pointer({ type: 'down', x: 8, y: 0, button: 'left', in: 'row' })
  await band.pointer({ type: 'up', x: 1, y: 0, button: 'left', in: 'row' })
  expect(ran).toHaveLength(1)
  await band.unmount()
})

test('a chip whose command its mod hides is not drawn, and comes back when the mod offers the command again', async ($, on) => {
  twoMods(on)
  on('command.describe', async (_$, e) => ({ description: e.description, argumentHint: e.argumentHint, isHidden: e.isHidden }))
  const describeInbox = (isHidden: boolean) =>
    $.command.describe({ command: 'inbox', description: 'Inbox', isHidden, immediate: false, provider: { plugin: 'mike-mail', tier: 'user' } })
  const band = await mountHint($)
  expect(await band.find({ type: 'Text', text: ' Inbox ', in: 'row' })).toBeDefined()
  expect(await describeInbox(true)).toMatchObject({ isHidden: true })
  expect(await band.find({ type: 'Text', text: ' Inbox ', in: 'row' })).toBeUndefined()
  expect(await band.find({ type: 'Text', text: ' Mods ', in: 'row' })).toBeDefined()
  expect(await band.find({ type: 'Text', text: ' Hide ', in: 'row' })).toBeDefined()
  await describeInbox(false)
  expect(await band.find({ type: 'Text', text: ' Inbox ', in: 'row' })).toBeDefined()
  await band.unmount()
})

test('which chips are drawn follows the hidden commands, by command name', () => {
  const buttons = [{ label: 'Fleet', command: '/fleet-comms' }, { label: 'Mods', command: '/mods' }]
  expect(commandName('/mods buttons')).toBe('mods')
  expect(shownButtons(buttons, ['fleet-comms'])).toEqual([{ label: 'Mods', command: '/mods' }])
  expect(withHidden([], 'fleet-comms', true)).toEqual(['fleet-comms'])
  expect(withHidden(['fleet-comms'], 'fleet-comms', true)).toBeNull()
  expect(withHidden(['fleet-comms'], 'fleet-comms', false)).toEqual([])
  expect(withHidden([], 'mods', false)).toBeNull()
})

test("a status from a mod with a chip is kept and its own line cleared; one from a mod without a chip, or while the row is hidden, passes through", () => {
  const labels = { 'what-changed': 'Changes', mike: 'Sessions' }
  const first = takeStatus({}, 'what-changed', 'changes: 2 of yours uncommitted', labels, true)
  expect(first).toEqual({ statuses: { 'what-changed': 'changes: 2 of yours uncommitted' }, pass: false })
  const same = takeStatus(first.statuses!, 'what-changed', 'changes: 2 of yours uncommitted', labels, true)
  expect(same).toEqual({ statuses: null, pass: false })
  const cleared = takeStatus(first.statuses!, 'what-changed', undefined, labels, true)
  expect(cleared).toEqual({ statuses: {}, pass: false })
  expect(takeStatus({}, 'what-changed', undefined, labels, true)).toEqual({ statuses: null, pass: false })
  expect(takeStatus({}, 'java-guard', 'refused 1', labels, true)).toEqual({ statuses: null, pass: true })
  expect(takeStatus({}, 'engine', 'anything', labels, true)).toEqual({ statuses: null, pass: true })
  expect(takeStatus({}, 'mike', '3 changes by others', labels, false)).toEqual({ statuses: null, pass: true })
})

test('a post the module did not make runs nothing', async ($, on) => {
  twoMods(on)
  const ran: string[] = []
  on('command.run', async (_$, e, next) => {
    ran.push(e.command)
    return next(e)
  })
  const band = await mountHint($)
  await band.post({ press: '/anything' }, { in: 'row' })
  await band.post({ press: 42 }, { in: 'row' })
  await band.post('text', { in: 'row' })
  expect(ran).toEqual([])
  await band.unmount()
})

test('the Hide chip gives the hint line back to Claude Code, and /mods buttons brings the chips back', async ($, on) => {
  twoMods(on)
  on('ui.render', async ($, e) => $.ui.resolve(e).Text({ children: 'ENGINE DRAWS THIS' }))
  const toasts: string[] = []
  on('ui.toast', async (_$, e) => {
    toasts.push(e.text)
    return { value: undefined }
  })
  const band = await mountHint($)
  expect(await band.find({ text: 'ENGINE DRAWS THIS' })).toBeUndefined()
  await band.pointer({ type: 'down', x: 15, y: 0, button: 'left', in: 'row' })
  await band.pointer({ type: 'up', x: 15, y: 0, button: 'left', in: 'row' })
  expect(toasts).toEqual(['The buttons under the prompt are hidden. /mods buttons brings them back.'])
  expect(await band.find({ type: 'Client', key: 'row' })).toBeUndefined()
  expect(await band.find({ text: 'ENGINE DRAWS THIS' })).toBeDefined()
  const back = await $.command.run({ command: 'mods', args: 'buttons' })
  expect(back.text).toBe('The buttons under the prompt are back.')
  expect(await band.find({ type: 'Client', key: 'row' })).toBeDefined()
  await band.unmount()
})

test('on the desktop the same chips draw, as the surface styles them', async ($, on) => {
  twoMods(on)
  const band = await mountHint($, 'desktop')
  expect(await band.find({ type: 'Client', key: 'row' })).toBeDefined()
  expect(await band.find({ type: 'Text', text: ' Mods ', in: 'row' })).toMatchObject({ props: { backgroundColor: LOOK.rest.bg } })
  await band.unmount()
})

test('the Mods pane has the button that hides or shows the row', async ($, on) => {
  twoMods(on)
  on('ui.render', async ($, e) => $.ui.resolve(e).Text({ children: 'ENGINE DRAWS THIS' }))
  await $.command.run({ command: 'mods', args: '' })
  const pane = await $.ui.mount({ plugin: 'mods', surface: 'terminal', component: 'Pane', props: PANE_PROPS, requestId: 'mods' })
  expect(await pane.find({ key: 'buttons-toggle' })).toMatchObject({ props: { label: 'Hide the buttons under the prompt' } })
  await pane.press({ key: 'buttons-toggle' })
  expect(await pane.find({ key: 'buttons-toggle' })).toMatchObject({ props: { label: 'Show the buttons under the prompt' } })
  const band = await mountHint($)
  expect(await band.find({ type: 'Client', key: 'row' })).toBeUndefined()
  await band.unmount()
})

test('the filter narrows the table', async ($, on) => {
  twoMods(on)
  await $.command.run({ command: 'mods', args: '' })
  const pane = await $.ui.mount({ plugin: 'mods', surface: 'terminal', component: 'Pane', props: PANE_PROPS, requestId: 'mods' })
  await pane.input({ key: 'filter', text: 'nothing-like-this', kind: 'change' })
  expect(await pane.find({ text: /No mod matches/ })).toBeDefined()
})

test('the pane says where each mod loads from, and that it is global', async ($, on) => {
  twoMods(on)
  on('fs.stat', async (_$, e) => ({
    value: { kind: 'other', size: 0, mtimeMs: 0, isLink: true, realPath: e.path === '/m/mods' ? '/repo/temet-proxy/mods/mods' : '/m/mail' },
  }))
  await $.command.run({ command: 'mods', args: '' })
  const pane = await $.ui.mount({ plugin: 'mods', surface: 'terminal', component: 'Pane', props: PANE_PROPS, requestId: 'mods' })
  expect(await pane.find({ text: /^loads from temet-proxy\/mods\/mods$/ })).toBeDefined()
  expect(await pane.find({ text: /^loads from \/m\/mail$/ })).toBeDefined()
  expect(await pane.find({ text: /^v0\.2\.0 · global$/ })).toBeDefined()
})
