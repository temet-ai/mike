export type ModOffers = {
  commands?: string[]
  panes?: string[]
  bands?: string[]
  status?: string[]
  acts?: string[]
}

export type ModEntry = {
  name: string
  version?: string
  tagline?: string
  description?: string
  family?: string
  offers?: ModOffers
  folder: string
  linksTo?: string
  scope: 'global' | 'project'
}

export type ModListing = { entries: ModEntry[]; home?: string; project?: string }

const text = (value: unknown): string | undefined =>
  typeof value === 'string' && value.length > 0 ? value : undefined

const strings = (value: unknown): string[] | undefined => {
  if (!Array.isArray(value)) return undefined
  const list = value.filter((item): item is string => typeof item === 'string' && item.length > 0)
  return list.length > 0 ? list : undefined
}

const parsed = (source: string | undefined): Record<string, unknown> => {
  if (source === undefined) return {}
  try {
    const value: unknown = JSON.parse(source)
    return value !== null && typeof value === 'object' ? (value as Record<string, unknown>) : {}
  } catch {
    return {}
  }
}

const offersOf = (value: unknown): ModOffers | undefined => {
  if (value === null || typeof value !== 'object') return undefined
  const raw = value as Record<string, unknown>
  const offers: ModOffers = {
    commands: strings(raw.commands),
    panes: strings(raw.panes),
    bands: strings(raw.bands),
    status: strings(raw.status),
    acts: strings(raw.acts),
  }
  return Object.values(offers).some(Boolean) ? offers : undefined
}

export function entryOf(
  folder: string,
  manifestText: string | undefined,
  selfText: string | undefined,
  linksTo: string | undefined,
  scope: ModEntry['scope'],
): ModEntry {
  const manifest = parsed(manifestText)
  const self = parsed(selfText)
  return {
    name: text(manifest.name) ?? folder.split('/').filter(Boolean).pop() ?? folder,
    version: text(manifest.version),
    tagline: text(self.tagline),
    description: text(manifest.description),
    family: text(self.family),
    offers: offersOf(self.offers),
    folder,
    linksTo,
    scope,
  }
}

export function pluginDirsOf(value: string | undefined): string[] {
  if (!value) return []
  const seen = new Set<string>()
  return value
    .split(':')
    .map(dir => dir.trim().replace(/\/+$/, ''))
    .filter(dir => dir.length > 0 && !seen.has(dir) && (seen.add(dir), true))
}

export function modsNoteBody(listing: ModListing) {
  const count = listing.entries.length
  return {
    kind: 'mods',
    title: `Loaded mods: ${count}`,
    body: listing.entries.map(entry => `${entry.name}${entry.version ? ` ${entry.version}` : ''}`).join(', '),
    data: listing,
  }
}
