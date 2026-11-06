import { atom, read, update } from 'claude-code'
import type { EngineInterface, Register } from 'claude-code'

import type { Chip } from './row'
import { rowWidth } from './row'

export type RowButton = { label: string; command: string }

export type Offers = {
  commands?: string[]
  panes?: string[]
  bands?: string[]
  status?: string[]
  acts?: string[]
  buttons?: RowButton[]
}

export type Source = 'repo' | 'folder'

export type Scope = 'global' | 'project'

export type ModEntry = {
  name: string
  version?: string
  tagline?: string
  description?: string
  family?: string
  offers?: Offers
  folder: string
  linksTo?: string
  scope: Scope
}

export type Listing = { entries: ModEntry[]; others: string[]; home?: string; project?: string }

export type Group = { title: string; note: string; entries: ModEntry[] }

export type Seen = { where: 'pane' | 'band' | 'status'; what: string }

export type Columns = { type: string[]; see: Seen[]; acts: string[] }

type Manifest = {
  name?: unknown
  version?: unknown
  description?: unknown
}

type SelfDescription = { tagline?: unknown; family?: unknown; offers?: unknown }

const PANE = 'mods'
const filter = atom({ plugin: 'mods', key: 'filter' } as const, '')
const buttonsShown = atom({ plugin: 'mods', key: 'buttonsShown' } as const, true)
const statuses = atom({ plugin: 'mods', key: 'statuses' } as const, {} as Record<string, string>)
const hiddenCommands = atom({ plugin: 'mods', key: 'hiddenCommands' } as const, [] as string[])
const PILL_ROOM = 30

function strings(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) return undefined
  const list = value.filter((item): item is string => typeof item === 'string' && item.length > 0)
  return list.length > 0 ? list : undefined
}

function text(value: unknown): string | undefined {
  return typeof value === 'string' && value.length > 0 ? value : undefined
}

function buttonsOf(value: unknown): RowButton[] | undefined {
  if (!Array.isArray(value)) return undefined
  const list = value.flatMap((item): RowButton[] => {
    if (item === null || typeof item !== 'object') return []
    const { label, command } = item as Record<string, unknown>
    const isNamed = typeof label === 'string' && label.length > 0
    const isCommand = typeof command === 'string' && command.startsWith('/') && command.length > 1
    return isNamed && isCommand ? [{ label, command }] : []
  })
  return list.length > 0 ? list : undefined
}

export function readOffers(value: unknown): Offers | undefined {
  if (value === null || typeof value !== 'object') return undefined
  const raw = value as Record<string, unknown>
  const offers: Offers = {
    commands: strings(raw.commands),
    panes: strings(raw.panes),
    bands: strings(raw.bands),
    status: strings(raw.status),
    acts: strings(raw.acts),
    buttons: buttonsOf(raw.buttons),
  }
  return Object.values(offers).some(Boolean) ? offers : undefined
}

export function rowOf(listing: Listing): RowButton[] {
  return listing.entries.flatMap(entry => entry.offers?.buttons ?? [])
}

export function commandName(command: string): string {
  return command.replace(/^\//, '').split(' ')[0] ?? ''
}

// A chip is drawn only while its command is offered: a mod hides its command
// (command.describe answering isHidden) when it does not apply to the session.
export function shownButtons(buttons: RowButton[], hidden: readonly string[]): RowButton[] {
  return buttons.filter(b => !hidden.includes(commandName(b.command)))
}

// The hidden list after one command.describe answer, or null when unchanged.
export function withHidden(hidden: readonly string[], command: string, isHidden: boolean): string[] | null {
  if (hidden.includes(command) === isHidden) return null
  return isHidden ? [...hidden, command] : hidden.filter(name => name !== command)
}

export const HIDE_COMMAND = '/mods buttons'

export function chipsOf(buttons: RowButton[]): Chip[] {
  return [...buttons.map(b => ({ label: b.label, command: b.command })), { label: 'Hide', command: HIDE_COMMAND, dim: true }]
}

export function labelsOf(listing: Listing): Record<string, string> {
  const labels: Record<string, string> = {}
  for (const entry of listing.entries) {
    const first = entry.offers?.buttons?.[0]
    if (first !== undefined) labels[entry.name] = first.label
  }
  return labels
}

export function statusWords(current: Record<string, string>, labels: Record<string, string>): string[] {
  return Object.entries(labels).flatMap(([plugin, label]) => {
    const text = current[plugin]?.trim()
    if (text === undefined || text === '') return []
    const prefix = `${label.toLowerCase()}:`
    const bare = text.toLowerCase().startsWith(prefix) ? text.slice(prefix.length).trimStart() : text
    return [`${label}: ${bare}`]
  })
}

function cut(s: string, n: number): string {
  if (s.length <= n) return s
  return n < 2 ? '' : `${s.slice(0, n - 1)}…`
}

export type Footer = { statuses: string; hint: string }

export function footerText(words: string[], hint: string, columns: number | undefined, chips: Chip[]): Footer {
  const statuses = words.join(' · ')
  if (columns === undefined) return { statuses, hint }
  const hintRoom = Math.max(0, columns - PILL_ROOM - rowWidth(chips) - 2)
  const statusRoom = Math.max(0, columns - PILL_ROOM - 2)
  return { statuses: cut(statuses, statusRoom), hint: hintRoom >= 6 ? cut(hint, hintRoom) : '' }
}

export type StatusTake = { statuses: Record<string, string> | null; pass: boolean }

export function takeStatus(
  current: Record<string, string>,
  who: string,
  text: string | undefined,
  labels: Record<string, string>,
  shown: boolean,
): StatusTake {
  if (labels[who] === undefined || !shown) return { statuses: null, pass: true }
  const changed = text === undefined ? who in current : current[who] !== text
  if (!changed) return { statuses: null, pass: false }
  const next = { ...current }
  if (text === undefined) delete next[who]
  else next[who] = text
  return { statuses: next, pass: false }
}


export function pressedCommand(data: unknown, chips: Chip[]): string | null {
  if (data === null || typeof data !== 'object') return null
  const press = (data as Record<string, unknown>).press
  if (typeof press !== 'string') return null
  return chips.some(c => c.command === press) ? press : null
}

function parsed<T extends object>(source: string | undefined): Partial<T> {
  if (source === undefined) return {}
  try {
    const value: unknown = JSON.parse(source)
    return value !== null && typeof value === 'object' ? (value as Partial<T>) : {}
  } catch {
    return {}
  }
}

export function entryFrom(
  folder: string,
  manifestText: string | undefined,
  linksTo?: string,
  selfText?: string,
  scope: Scope = 'global',
): ModEntry {
  const fallback = folder.split('/').filter(Boolean).pop() ?? folder
  const manifest = parsed<Manifest>(manifestText)
  const self = parsed<SelfDescription>(selfText)
  return {
    name: text(manifest.name) ?? fallback,
    version: text(manifest.version),
    tagline: text(self.tagline),
    description: text(manifest.description),
    family: text(self.family),
    offers: readOffers(self.offers),
    folder,
    linksTo,
    scope,
  }
}

export function groupsOf(listing: Listing): Group[] {
  const groups: Group[] = []
  const project = listing.entries.filter(entry => entry.scope === 'project')
  if (project.length > 0) {
    groups.push({ title: `This project${listing.project ? ` · ${listing.project}` : ''}`, note: 'loads only here', entries: sorted(project) })
  }
  const global = listing.entries.filter(entry => entry.scope === 'global')
  const families = [...new Set(global.map(entry => entry.family).filter((f): f is string => f !== undefined))].sort()
  for (const family of families) {
    groups.push({ title: family, note: 'family · every project', entries: sorted(global.filter(entry => entry.family === family)) })
  }
  const loose = global.filter(entry => entry.family === undefined)
  if (loose.length > 0) groups.push({ title: 'Global', note: 'every project', entries: sorted(loose) })
  return groups
}

export function pluginDirs(value: string | undefined): string[] {
  if (!value) return []
  const seen = new Set<string>()
  return value
    .split(':')
    .map(dir => dir.trim().replace(/\/+$/, ''))
    .filter(dir => dir.length > 0 && !seen.has(dir) && (seen.add(dir), true))
}

export function sourceOf(entry: ModEntry): Source {
  return entry.linksTo !== undefined && /\/mods\/[^/]+$/.test(entry.linksTo) ? 'repo' : 'folder'
}

export function shortPath(path: string, home?: string): string {
  if (home && (path === home || path.startsWith(`${home}/`))) return `~${path.slice(home.length)}`
  return path
}

export function where(entry: ModEntry, home?: string): string {
  const target = entry.linksTo ?? entry.folder
  const inRepo = /([^/]+\/mods\/[^/]+)$/.exec(target)
  if (entry.linksTo !== undefined && inRepo) return inRepo[1]
  return shortPath(target, home)
}

export function columnsOf(entry: ModEntry): Columns {
  const offers = entry.offers ?? {}
  return {
    type: offers.commands ?? [],
    see: [
      ...(offers.panes ?? []).map(what => ({ where: 'pane' as const, what })),
      ...(offers.bands ?? []).map(what => ({ where: 'band' as const, what })),
      ...(offers.status ?? []).map(what => ({ where: 'status' as const, what })),
    ],
    acts: offers.acts ?? [],
  }
}

export function totals(entries: ModEntry[]): Columns {
  const all = entries.map(columnsOf)
  return { type: all.flatMap(c => c.type), see: all.flatMap(c => c.see), acts: all.flatMap(c => c.acts) }
}

export function matches(entry: ModEntry, query: string): boolean {
  const q = query.trim().toLowerCase()
  if (!q) return true
  const c = columnsOf(entry)
  const haystack = [entry.name, entry.tagline ?? '', entry.description ?? '', ...c.type, ...c.see.map(s => s.what), ...c.acts]
  return haystack.some(part => part.toLowerCase().includes(q))
}

export function headline(listing: Listing): string {
  const total = listing.entries.length + listing.others.length
  if (total === 0) return 'No mods are loaded in this session.'
  return `${total} mod${total === 1 ? '' : 's'} loaded in this session`
}

function sorted(entries: ModEntry[]): ModEntry[] {
  return [...entries].sort((a, b) => a.name.localeCompare(b.name))
}

export function describe(listing: Listing): string {
  const lines = [headline(listing)]
  for (const group of groupsOf(listing)) {
    lines.push('')
    lines.push(`== ${group.title} (${group.note})`)
    for (const entry of group.entries) {
      const c = columnsOf(entry)
      lines.push('')
      lines.push(`${entry.name}${entry.version ? ` ${entry.version}` : ''}  (${where(entry, listing.home)})`)
      lines.push(`  ${entry.tagline ?? entry.description ?? 'No description'}`)
      if (c.type.length > 0) lines.push(`  /command  ${c.type.join(', ')}`)
      if (c.see.length > 0) lines.push(`  produces  ${c.see.map(s => `${s.what} (${s.where})`).join(', ')}`)
      if (c.acts.length > 0) lines.push(`  auto      ${c.acts.join(', ')}`)
    }
  }
  if (listing.others.length > 0) {
    lines.push('')
    lines.push(`+ ${listing.others.length} more from a marketplace or --plugin-dir: ${[...listing.others].sort().join(', ')}`)
  }
  return lines.join('\n')
}

async function readEntry($: EngineInterface, folder: string, scope: Scope): Promise<ModEntry> {
  const stat = await $.fs.stat(folder, { resolve: true }).catch(() => undefined)
  const linksTo = stat?.isLink ? stat.realPath : undefined
  const manifest = await $.fs.read(`${folder}/.claude-plugin/plugin.json`).catch(() => undefined)
  const self = await $.fs.read(`${folder}/.claude-plugin/mod.json`).catch(() => undefined)
  return entryFrom(folder, manifest, linksTo, self, scope)
}

async function skillMods($: EngineInterface, skills: string | undefined): Promise<string[]> {
  if (!skills) return []
  const children = await $.fs.list(skills).catch(() => [])
  const folders = children.map(child => `${skills}/${child.name}`)
  const isMod = await Promise.all(folders.map(folder => $.fs.exists(`${folder}/.claude-plugin/plugin.json`).catch(() => false)))
  return folders.filter((_, i) => isMod[i])
}

let loaded: readonly string[] = []
type RowInfo = { buttons: RowButton[]; labels: Record<string, string> }
let rowCache: RowInfo | null = null
let rowFaulted = false

function rowInfoOf(listing: Listing): RowInfo {
  return { buttons: rowOf(listing), labels: labelsOf(listing) }
}

async function rowInfo($: EngineInterface): Promise<RowInfo> {
  if (rowCache === null) rowCache = rowInfoOf(await listMods($))
  return rowCache
}

async function shownRow($: EngineInterface): Promise<RowButton[]> {
  return shownButtons((await rowInfo($)).buttons, await read($, hiddenCommands))
}

async function noteHidden($: EngineInterface, command: string, isHidden: boolean): Promise<void> {
  const { buttons } = await rowInfo($)
  if (!buttons.some(b => commandName(b.command) === command)) return
  const changed = withHidden(await read($, hiddenCommands), command, isHidden)
  if (changed !== null) await update($, hiddenCommands, () => changed)
}

async function setShown($: EngineInterface, shown: boolean): Promise<void> {
  await update($, buttonsShown, () => shown)
  if (!shown) await update($, statuses, () => ({}))
}

async function runCommand($: EngineInterface, command: string): Promise<void> {
  const [name, ...rest] = command.replace(/^\//, '').split(' ')
  if (name === undefined || name === '') return
  const ran = await $.command.run({ command: name, args: rest.join(' ') }).catch(() => undefined)
  if (ran?.text) $.ui.toast(ran.text)
}

async function hideButtons($: EngineInterface): Promise<void> {
  await setShown($, false)
  $.ui.toast('Buttons hidden. /mods buttons brings them back.')
}

export const HIDDEN_WORDS = 'The buttons under the prompt are hidden. /mods buttons brings them back.'
export const SHOWN_WORDS = 'The buttons under the prompt are back.'

export async function listMods($: EngineInterface): Promise<Listing> {
  const home = await $.env.get('HOME')
  const root = await $.session.root().catch(() => undefined)
  const globalDirs = [
    ...pluginDirs(await $.env.get('CLAUDE_CODE_PLUGIN_DIRS')),
    ...(await skillMods($, home ? `${home}/.claude/skills` : undefined)),
  ]
  const projectDirs = root && root !== home ? await skillMods($, `${root}/.claude/skills`) : []
  const entries = [
    ...(await Promise.all(projectDirs.map(dir => readEntry($, dir, 'project')))),
    ...(await Promise.all(globalDirs.map(dir => readEntry($, dir, 'global')))),
  ].filter((entry, i, all) => all.findIndex(other => other.name === entry.name) === i)
  const named = new Set(entries.map(entry => entry.name))
  const others = loaded.filter(name => !named.has(name) && !name.includes('/'))
  const project = root?.split('/').filter(Boolean).pop()
  return { entries, others, home, project }
}

const TYPE = { fg: '#C9D0FA', bg: '#262A3D', head: 'suggestion' } as const
const SEE = { fg: '#E8C2B0', bg: '#3A2A22', dim: '#B07A62', head: 'claude' } as const
const ACTS = { fg: '#F0DCA0', bg: '#3A3220', head: 'warning' } as const
const ROW_REST = 'inactive'
const ROW_HOVER = 'claude'

function widths(body: number) {
  const name = Math.min(21, Math.max(15, Math.floor(body * 0.22)))
  const rest = Math.max(30, body - name - 6)
  return { name, type: Math.floor(rest * 0.28), see: Math.floor(rest * 0.42), acts: rest - Math.floor(rest * 0.28) - Math.floor(rest * 0.42) }
}

export const register: Register = on => {
  on('engine.create', async (_$, e, next) => {
    loaded = e.plugins
    return next(e)
  })

  on('session.start', async ($, e, next) => {
    rowCache = null
    await $.command.register({
      name: 'mods',
      description: 'Show the mods this session loaded: their commands, what they produce and what they do on their own',
      argumentHint: '[text | buttons]',
    })
    return next(e)
  })

  on('command.run', { command: 'mods' }, async ($, e) => {
    const args = (e.args ?? '').trim()
    if (args === 'buttons') {
      await setShown($, !(await read($, buttonsShown)))
      return { text: (await read($, buttonsShown)) ? SHOWN_WORDS : HIDDEN_WORDS }
    }
    const listing = await listMods($)
    rowCache = rowInfoOf(listing)
    if (args === 'text') return { text: describe(listing) }
    const opened = await $.ui.open({ id: PANE, title: 'Mods', focus: true, closeOnEscape: true })
    return { text: opened.isPlaced ? headline(listing) : describe(listing) }
  })

  // The engine raises this for every command it lists, again after any mod
  // invalidates it; the answer beneath says whether the command is offered.
  on('command.describe', async ($, e, next) => {
    const answer = await next(e)
    await noteHidden($, e.command, answer.isHidden).catch(() => undefined)
    return answer
  })

  on('ui.status', async ($, e, next) => {
    const { labels } = await rowInfo($)
    const take = takeStatus(await read($, statuses), next.origin.plugin, e.text, labels, await read($, buttonsShown))
    if (take.pass) return next(e)
    const changed = take.statuses
    if (changed !== null) await update($, statuses, () => changed)
    return next({ ...e, text: undefined })
  })

  on('ui.render', { component: 'PromptHint' }, async ($, e, next) => {
    if (!(await read($, buttonsShown))) return next(e)
    const { buttons: offered, labels } = await rowInfo($)
    if (offered.length === 0) return next(e)
    const buttons = await shownRow($)
    const { Box, Text, Button, Client } = $.ui.resolve(e)
    const chips = chipsOf(buttons)
    const tail = footerText(statusWords(await read($, statuses), labels), e.props.hint, e.viewport?.columns, chips)
    const plainRow = (
      <Box columnGap={1}>
        {buttons.map(b => (
          <Button key={`row-${b.command}`} label={b.label} variant="secondary" onPress={() => void runCommand($, b.command)} />
        ))}
        <Button key="row-hide" label="Hide" variant="secondary" dimColor onPress={() => void hideButtons($)} />
      </Box>
    )
    return (
      <Box flexDirection="column">
        <Box columnGap={2}>
          {rowFaulted ? plainRow : <Client key="row" module="./row.tsx" props={{ chips }} />}
          <Text dimColor>{tail.hint}</Text>
        </Box>
        {tail.statuses !== '' && <Text>{tail.statuses}</Text>}
      </Box>
    )
  })

  on('ui.message', async ($, e, next) => {
    if (e.element !== 'row') return next(e)
    const command = pressedCommand(e.data, chipsOf(await shownRow($)))
    if (command !== null) void runCommand($, command)
    return next(e)
  })

  on('ui.fault', async ($, e, next) => {
    if (e.element !== 'row') return next(e)
    rowFaulted = true
    $.ui.log(`mods: the button row's module failed (${e.phase}: ${e.reason}); drawing plain buttons instead`)
    return next(e)
  })

  on('ui.render', { component: 'Pane', requestId: PANE }, async ($, e) => {
    const { Box, Text, Button, Input } = $.ui.resolve(e)
    const listing = await listMods($)
    const query = await read($, filter)
    const rowShown = await read($, buttonsShown)
    const shown = sorted(listing.entries).filter(entry => matches(entry, query))
    const groups = groupsOf({ ...listing, entries: shown })
    const body = (e.props as { bodyColumns?: number }).bodyColumns ?? e.viewport?.columns ?? 80
    const w = widths(body - 6)

    const chip = (label: string, fg: string, bg: string, prefix?: string) => (
      <Text backgroundColor={bg} color={fg}>
        {' '}
        {prefix && <Text color={SEE.dim}>{prefix} </Text>}
        {label}{' '}
      </Text>
    )
    const none = <Text color="inactive">—</Text>

    const run = async (command: string) => {
      await $.ui.close({ id: PANE }).catch(() => undefined)
      await runCommand($, command)
    }

    const row = (entry: ModEntry) => {
      const c = columnsOf(entry)
      return (
        <Box
          key={`row-${entry.name}`}
          flexDirection="column"
          borderStyle="round"
          borderColor={ROW_REST}
          hover={{ borderColor: ROW_HOVER }}
          paddingX={1}
        >
          <Box>
            <Box width={w.name} flexDirection="column">
              <Text bold wrap="truncate-end">{entry.name}</Text>
              <Text dimColor wrap="truncate-end">{`v${entry.version ?? '?'} · ${entry.scope}`}</Text>
            </Box>
            <Box width={w.type} flexDirection="column">
              {c.type.length === 0
                ? none
                : c.type.map(command => (
                    <Button key={`run-${command}`} plain hover={{ underline: true, color: TYPE.head }} onPress={() => void run(command)}>
                      {command}
                    </Button>
                  ))}
            </Box>
            <Box width={w.see} flexWrap="wrap" columnGap={1}>
              {c.see.length === 0 ? none : c.see.map(s => chip(s.what, SEE.fg, SEE.bg, s.where))}
            </Box>
            <Box width={w.acts} flexWrap="wrap" columnGap={1}>
              {c.acts.length === 0 ? none : c.acts.map(a => chip(a, ACTS.fg, ACTS.bg))}
            </Box>
          </Box>
          <Text dimColor wrap="wrap">{entry.tagline ?? entry.description ?? 'Does not say what it does yet.'}</Text>
          <Text dimColor wrap="truncate-start">{`loads from ${where(entry, listing.home)}`}</Text>
        </Box>
      )
    }

    return (
      <Box flexDirection="column" paddingX={1}>
        <Box justifyContent="space-between" marginBottom={1}>
          <Text dimColor>{headline(listing)}</Text>
          <Input
            key="filter"
            label="⌕"
            placeholder="filter"
            value={query}
            submitLabel="done"
            onInput={value => void update($, filter, () => value)}
            onSubmit={value => void update($, filter, () => value)}
          />
        </Box>

        {listing.entries.length === 0 && listing.others.length === 0 && (
          <Box flexDirection="column" marginTop={1}>
            <Text bold>No mods in this session</Text>
            <Text dimColor>Put a mod's folder on CLAUDE_CODE_PLUGIN_DIRS and start a new session.</Text>
          </Box>
        )}

        {listing.entries.length > 0 && (
          <Box flexDirection="column">
            <Box paddingX={2}>
              <Box width={w.name}><Text bold>name</Text></Box>
              <Box width={w.type}><Text bold color={TYPE.head}>/command</Text></Box>
              <Box width={w.see}><Text bold color={SEE.head}>produces</Text></Box>
              <Box width={w.acts}><Text bold color={ACTS.head}>auto</Text></Box>
            </Box>
            {shown.length === 0 && <Text dimColor>No mod matches “{query}”.</Text>}
            {groups.map(group => (
              <Box flexDirection="column" marginTop={1}>
                <Text>
                  <Text bold color={group.entries[0]?.scope === 'project' ? 'success' : 'claude'}>{group.title}</Text>
                  <Text dimColor>{`  ${group.note} · ${group.entries.length} mod${group.entries.length === 1 ? '' : 's'}`}</Text>
                </Text>
                {group.entries.map(row)}
              </Box>
            ))}
          </Box>
        )}

        {listing.others.length > 0 && (
          <Text dimColor wrap="wrap">
            + {String(listing.others.length)} more from a marketplace: {[...listing.others].sort().join(' · ')}
          </Text>
        )}

        <Box columnGap={2} marginTop={1}>
          <Text dimColor>click a <Text color={TYPE.head}>/command</Text> to run it</Text>
          <Text dimColor><Text color="text">esc</Text> close</Text>
          <Button
            key="buttons-toggle"
            label={rowShown ? 'Hide the buttons under the prompt' : 'Show the buttons under the prompt'}
            variant="secondary"
            onPress={() => void setShown($, !rowShown)}
          />
        </Box>
      </Box>
    )
  })
}
