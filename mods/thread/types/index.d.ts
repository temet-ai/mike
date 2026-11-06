export type BoardRow = {
  slug: string
  title: string | null
  status: string | null
  tldr: string | null
  next_up: string | null
  parse_error: string | null
  updated_at: string | null
}

export type BoardFault = { at: string; body: string }

export type BoardState = {
  rows: BoardRow[] | null
  total: number
  page: number
  all: boolean
  fault: BoardFault | null
}

declare module 'claude-code' {
  interface PluginState {
    thread: { board: BoardState }
  }
}
