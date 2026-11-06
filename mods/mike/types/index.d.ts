export type Seat = {
  id: string
  label: string
  role: string | null
  liveness: string
  state: string
  goal: string | null
  isSelf: boolean
}

export type Roster = { seats: Seat[]; at: string; error: string | null }

declare module 'claude-code' {
  interface PluginState {
    mike: { roster: Roster }
  }
}
