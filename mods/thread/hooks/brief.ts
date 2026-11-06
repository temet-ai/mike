export type ThreadRow = { slug: string; title: string | null; status: string | null }
export type ThreadListing = { threads: ThreadRow[]; total: number }

export const BRIEF_LIMIT_BYTES = 2048
export const BRIEF_STATUSES = 'active,implementing,review'
export const BRIEF_HEADING =
  "This project's open work threads, held in mike (they replace worklog.md files). Read one with work_thread_get, record progress with work_thread_checkpoint, or use /thread:get and /thread:checkpoint:"

const bytes = (text: string) => new TextEncoder().encode(text).length

/** True when the tether mod is among the session's mod folders: tether's
 * threads feature then states the rule and carries the open threads. */
export function tetherLoaded(pluginDirs: string | undefined): boolean {
  return (pluginDirs ?? '')
    .split(':')
    .some(dir => dir.replace(/\/+$/, '').split('/').pop() === 'tether')
}

export function isListing(value: unknown): value is ThreadListing {
  if (typeof value !== 'object' || value === null) return false
  const body = value as { threads?: unknown; total?: unknown }
  return Array.isArray(body.threads) && typeof body.total === 'number'
}

export function rowLine(row: ThreadRow): string {
  const title = (row.title ?? '').replace(/\s+/g, ' ').trim()
  return `- ${row.slug} [${row.status ?? 'unknown'}]${title === '' ? '' : ' ' + title}`
}

function restLine(rest: number): string {
  return `${rest} more: work_thread_list with status ${BRIEF_STATUSES}`
}

export function threadsBrief(listing: ThreadListing, limit = BRIEF_LIMIT_BYTES): string | null {
  if (listing.total === 0 || listing.threads.length === 0) return null
  const open = `<system-reminder>\n${BRIEF_HEADING}\n`
  const close = '</system-reminder>'
  const lines: string[] = []
  for (const row of listing.threads) {
    const next = [...lines, rowLine(row)]
    const rest = listing.total - next.length
    const tail = rest > 0 ? restLine(rest) + '\n' : ''
    if (bytes(open + next.join('\n') + '\n' + tail + close) > limit) break
    lines.push(rowLine(row))
  }
  const rest = listing.total - lines.length
  const body = [...lines, ...(rest > 0 ? [restLine(rest)] : [])]
  return open + body.join('\n') + '\n' + close
}
