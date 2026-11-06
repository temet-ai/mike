export type BoardRow = {
  slug: string
  title: string | null
  status: string | null
  tldr: string | null
  next_up: string | null
  parse_error: string | null
  updated_at: string | null
}

export const PAGE_SIZE = 50
export const OPEN_STATUSES = 'active,implementing,review'
export const WIDE_COLUMNS = 72

/** Status colour keys, shared by the board and the card. */
export const STATUS_COLOR: Record<string, string> = {
  implementing: 'claude',
  review: 'warning',
  active: 'suggestion',
  parked: 'inactive',
  done: 'success',
}

export const GROUP_ORDER = ['implementing', 'review', 'active', 'parked', 'done']

export const GROUP_NOTE: Record<string, string> = {
  implementing: 'Building something specific; needs acceptance criteria',
  review: 'Criteria met and seen working for real',
  active: 'In progress, no acceptance bar yet',
  parked: 'Paused',
  done: 'Complete',
}

export const NO_STATUS = 'no status'

export const statusWord = (status: string | null | undefined): string =>
  status === null || status === undefined || status === '' ? NO_STATUS : status

export const statusColor = (status: string | null | undefined): string =>
  (status !== null && status !== undefined && STATUS_COLOR[status]) || 'subtle'

export function listUrl(inspect: string, root: string, all: boolean, page: number): string {
  const status = all ? '' : `&status=${OPEN_STATUSES}`
  return `${inspect}/api/work-threads?project=${encodeURIComponent(root)}${status}&limit=${PAGE_SIZE}&offset=${page * PAGE_SIZE}`
}

const text = (v: unknown): string | null => (typeof v === 'string' ? v : null)

export function readListing(value: unknown): { rows: BoardRow[]; total: number } | null {
  if (typeof value !== 'object' || value === null) return null
  const body = value as { threads?: unknown; total?: unknown }
  if (!Array.isArray(body.threads) || typeof body.total !== 'number') return null
  const rows = body.threads.flatMap((item): BoardRow[] => {
    if (typeof item !== 'object' || item === null) return []
    const r = item as Record<string, unknown>
    if (typeof r.slug !== 'string') return []
    return [
      {
        slug: r.slug,
        title: text(r.title),
        status: text(r.status),
        tldr: text(r.tldr),
        next_up: text(r.next_up),
        parse_error: text(r.parse_error),
        updated_at: text(r.updated_at),
      },
    ]
  })
  return { rows, total: body.total }
}

export type Group = { status: string; rows: BoardRow[] }

const stamp = (r: BoardRow): number => {
  const t = r.updated_at === null ? NaN : Date.parse(r.updated_at)
  return Number.isNaN(t) ? 0 : t
}

/** One group per status present, in the fixed order, newest first inside. */
export function groupRows(rows: BoardRow[]): Group[] {
  const known = new Set(GROUP_ORDER)
  const keys = [...GROUP_ORDER, ...rows.map(r => statusWord(r.status)).filter(s => !known.has(s) && s !== NO_STATUS), NO_STATUS]
  const seen = new Set<string>()
  return keys
    .filter(k => (seen.has(k) ? false : (seen.add(k), true)))
    .flatMap((status): Group[] => {
      const inGroup = rows.filter(r => statusWord(r.status) === status).sort((a, b) => stamp(b) - stamp(a))
      return inGroup.length > 0 ? [{ status, rows: inGroup }] : []
    })
}

export function age(iso: string | null, now: number): string {
  const t = iso === null ? NaN : Date.parse(iso)
  if (Number.isNaN(t)) return ''
  const s = Math.max(0, Math.floor((now - t) / 1000))
  if (s < 60) return 'just now'
  if (s < 3600) return `${Math.floor(s / 60)}m ago`
  if (s < 86_400) return `${Math.floor(s / 3600)}h ago`
  if (s < 7 * 86_400) return `${Math.floor(s / 86_400)}d ago`
  return (iso ?? '').slice(0, 10)
}

/** The folder name of a session root. */
export const folderName = (root: string): string => root.split('/').filter(Boolean).pop() ?? root

export function pageRange(page: number, shown: number, total: number): string {
  return `${page * PAGE_SIZE + 1}–${page * PAGE_SIZE + shown} of ${total}`
}

export const hasNext = (page: number, total: number): boolean => (page + 1) * PAGE_SIZE < total

export function clock(ms: number): string {
  const d = new Date(ms)
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

/** `/thread:<verb> <slug>` for a button; the slug is the only argument. */
export const commandFor = (verb: 'get' | 'checkpoint' | 'new', slug?: string): { command: string; args: string } => ({
  command: `thread:${verb}`,
  args: slug ?? '',
})
