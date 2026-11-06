import { atom, read, update } from 'claude-code'
import type { EngineInterface, Register } from 'claude-code'

import type { BoardFault, BoardRow, BoardState } from '../types'
import {
  GROUP_NOTE,
  WIDE_COLUMNS,
  age,
  clock,
  commandFor,
  folderName,
  groupRows,
  hasNext,
  listUrl,
  pageRange,
  readListing,
  statusColor,
} from './board-data'

const INSPECT = 'http://127.0.0.1:9989'
const PANE = 'thread-board'
const COMMAND = 'thread-board'
const POLL_MS = 15_000

const EMPTY: BoardState = { rows: null, total: 0, page: 0, all: false, fault: null }
const board = atom({ plugin: 'thread', key: 'board' } as const, EMPTY)

let polling = false
let nowMs = 0

function quiet() {}

async function load($: EngineInterface): Promise<void> {
  const state = await read($, board)
  const root = await $.session.root()
  let fault: BoardFault | null = null
  let got: { rows: BoardRow[]; total: number } | null = null
  const at = clock(await $.clock.now())
  try {
    const res = await $.http.fetch(listUrl(INSPECT, root, state.all, state.page))
    if (res.ok) {
      got = readListing(JSON.parse(res.text))
      if (got === null) fault = { at, body: 'mike answered, but not with a thread list.' }
    } else {
      fault = { at, body: res.text }
    }
  } catch {
    fault = { at, body: '' }
  }
  await update($, board, s => (got === null ? { ...s, fault } : { ...s, rows: got.rows, total: got.total, fault: null }))
}

function startPolling($: EngineInterface): void {
  if (polling) return
  polling = true
  $.clock.every(POLL_MS, () => {
    void (async () => {
      const open = (await $.ui.panes()).some(p => p.id === PANE)
      if (open) await load($)
    })().catch(quiet)
  })
}

/** Called from session.start: declares /thread-board (a colon is refused in a command name). */
export async function startBoard($: EngineInterface): Promise<void> {
  polling = false
  try {
    await $.command.register({ name: COMMAND, description: "Open the project's work threads in a pane" })
  } catch (err) {
    $.ui.log(`/${COMMAND} was refused: ${String(err)}`)
  }
}

async function runCommand($: EngineInterface, spec: { command: string; args: string }): Promise<void> {
  await $.ui.close({ id: PANE }).catch(quiet)
  const ran = await $.command.run(spec).catch(() => undefined)
  if (ran?.text) $.ui.toast(ran.text)
}

async function setView($: EngineInterface, change: (s: BoardState) => BoardState): Promise<void> {
  await update($, board, change)
  await load($)
}

async function openBoard($: EngineInterface): Promise<void> {
  await update($, board, s => ({ ...s, page: 0, all: false }))
  await $.ui.open({ id: PANE, title: 'Threads', focus: true, closeOnEscape: true })
  startPolling($)
  void load($).catch(quiet)
}

export const register: Register = on => {
  on('command.run', { command: COMMAND }, async $ => {
    await openBoard($)
    return {}
  })

  on('ui.render', { component: 'Pane', requestId: PANE }, async ($, e) => {
    const { Box, Text, Button } = $.ui.resolve(e)
    const s = await read($, board)
    const root = await $.session.root()
    const folder = folderName(root)
    nowMs = await $.clock.now()
    const columns = (e.props as { bodyColumns?: number }).bodyColumns ?? e.viewport?.columns ?? 80
    const wide = columns >= WIDE_COLUMNS
    const labelWidth = wide ? 8 : 7
    const down = s.fault !== null && s.rows === null
    const shown = s.rows ?? []

    const header = (
      <Box flexDirection={wide ? 'row' : 'column'} justifyContent="space-between">
        <Text dimColor>{`${s.all ? 'All' : 'Open'} threads in ${folder}`}</Text>
        <Box columnGap={1} flexWrap="wrap">
          <Button key="new" variant="secondary" label="New thread" hotkey="n" onPress={() => void runCommand($, commandFor('new'))} />
          <Button
            key="all"
            variant="secondary"
            label={s.all ? 'Open only' : 'Parked and done too'}
            hotkey="a"
            onPress={() => void setView($, x => ({ ...x, all: !x.all, page: 0 })).catch(quiet)}
          />
          {down ? (
            <Button key="retry" variant="primary" label="Retry" hotkey="r" onPress={() => void load($).catch(quiet)} />
          ) : (
            <Button key="refresh" variant="secondary" label="Refresh" hotkey="r" onPress={() => void load($).catch(quiet)} />
          )}
        </Box>
      </Box>
    )

    const line = (label: string, value: string, color?: string) => (
      <Box>
        <Box width={labelWidth} flexShrink={0}>
          <Text dimColor>{label}</Text>
        </Box>
        <Text wrap="wrap" color={color}>
          {value}
        </Text>
      </Box>
    )

    let hotkeys = 0
    const row = (r: BoardRow) => {
      const title = r.title ?? r.slug
      const hotkey = hotkeys < 9 ? String(++hotkeys) : undefined
      return (
        <Box
          key={`row-${r.slug}`}
          flexDirection="column"
          borderStyle="round"
          borderColor={r.parse_error !== null ? 'error' : 'subtle'}
          hover={{ borderColor: 'suggestion' }}
          paddingX={1}
        >
          <Box justifyContent="space-between">
            <Box columnGap={1} flexShrink={1}>
              <Button key={`open-${r.slug}`} plain hotkey={hotkey} hover={{ underline: true }} label={title} onPress={() => void runCommand($, commandFor('get', r.slug))} />
              {title !== r.slug && <Text dimColor>{r.slug}</Text>}
            </Box>
            <Box flexShrink={0}>
              <Text dimColor>{age(r.updated_at, nowMs)}</Text>
            </Box>
          </Box>
          {line('for', r.tldr ?? '')}
          {r.next_up === null || r.next_up === '' ? (
            <Box>
              <Box width={labelWidth} flexShrink={0}>
                <Text dimColor>next</Text>
              </Box>
              <Text color="inactive">not written yet</Text>
            </Box>
          ) : (
            line('next', r.next_up)
          )}
          {r.parse_error !== null && line('import', r.parse_error, 'error')}
          <Box>
            <Button key={`checkpoint-${r.slug}`} variant="secondary" label="Checkpoint" onPress={() => void runCommand($, commandFor('checkpoint', r.slug))} />
          </Box>
        </Box>
      )
    }

    const groups = groupRows(shown).map(g => (
      <Box key={`group-${g.status}`} flexDirection="column" marginTop={1}>
        <Text>
          <Text bold color={statusColor(g.status)}>
            {g.status}
          </Text>
          {wide && GROUP_NOTE[g.status] !== undefined && <Text dimColor>{`  ${GROUP_NOTE[g.status]}`}</Text>}
        </Text>
        {g.rows.map(row)}
      </Box>
    ))

    const prev = s.page > 0
    const next = hasNext(s.page, s.total)

    return (
      <Box flexDirection="column" paddingX={1}>
        {header}
        {s.rows === null && s.fault === null && (
          <Box marginTop={1}>
            <Text dimColor>Asking mike for this project's threads.</Text>
          </Box>
        )}
        {down && (
          <Box flexDirection="column" marginTop={1}>
            <Text bold color="error">
              mike is not answering at 127.0.0.1:9989
            </Text>
            <Text dimColor wrap="wrap">
              Threads live in mike's database. Start the stack with ./scripts/start-proxy.sh, then retry.
            </Text>
            {s.fault !== null && s.fault.body !== '' && <Text wrap="wrap">{s.fault.body}</Text>}
          </Box>
        )}
        {!down && s.fault !== null && (
          <Box marginTop={1}>
            <Text color="warning">{`mike stopped answering at ${s.fault.at}; this is the list from then.`}</Text>
          </Box>
        )}
        {s.rows !== null && shown.length === 0 && (
          <Box flexDirection="column" marginTop={1}>
            <Text bold>{`${s.all ? 'No threads in' : 'No open threads in'} ${folder}`}</Text>
            {!s.all && <Text dimColor>Start one with New thread, or press a to see parked and done threads.</Text>}
          </Box>
        )}
        {groups}
        {s.total > 50 && (
          <Box marginTop={1} columnGap={1}>
            <Text dimColor>{pageRange(s.page, shown.length, s.total)}</Text>
            {prev && (
              <Button key="prev" variant="secondary" label="Previous page" hotkey="p" onPress={() => void setView($, x => ({ ...x, page: x.page - 1 })).catch(quiet)} />
            )}
            {next && (
              <Button key="next" variant="secondary" label="Next page" hotkey="f" onPress={() => void setView($, x => ({ ...x, page: x.page + 1 })).catch(quiet)} />
            )}
          </Box>
        )}
        <Box marginTop={1}>
          <Text dimColor>click a thread to open it · Tab moves between buttons · esc closes</Text>
        </Box>
      </Box>
    )
  })
}
