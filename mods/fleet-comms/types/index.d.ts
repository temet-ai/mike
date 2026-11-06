export type FleetMessage = {
  id: string
  at: string
  from: string
  to: string
  body: string
}

export type FleetThread = {
  threadUuid: string
  slug: string
  state: string
  subject: string
  messageCount: number
  messages: FleetMessage[]
  ownName: string
  seatNames: string[]
}

export type Link = { failedAt: string | null; goodAt: string | null }

export type FleetVerdict = { at: string; verdict: string; title: string }

export type FleetTask = {
  id: string
  name: string
  discipline: string
  criteria: string[]
  seat: string | null
  state: string
  passes: number
  fails: number
  last: FleetVerdict | null
  ready: boolean
}

export type FleetSeat = {
  seat: string
  role: string
  model: string
  state: string
  session: string | null
  goal: string | null
  contextTokens: number | null
  requests: number
  lastRequestAt: string | null
  closed: boolean
  cache: { read: number; written: number; cold: boolean } | null
}

export type FleetCriterion = { id: string; tasks: number; passed: number; ready: number }

export type FleetObligation = { id: string; title: string; outcome: string; issue: string | null }

export type FleetNeed = { at: string; seat: string; title: string }

export type FleetPanel = {
  fleet: string
  slug: string
  state: string
  goal: string
  startedAt: string | null
  finishedAt: string | null
  now: string
  missing: { item: string; problem: string }[]
  accepted: boolean
  openObligations: FleetObligation[]
  criteria: FleetCriterion[]
  tasks: FleetTask[]
  seats: FleetSeat[]
  verdicts: FleetVerdict[]
  blocked: { seat: string; title: string }[]
  needsOperator: FleetNeed[]
  everySeatIdleSeconds: number | null
  isOwn: boolean
}

export type FleetStatusState = {
  panel: FleetPanel | null
  failedAt: string | null
  isLoaded: boolean
}

declare module 'claude-code' {
  interface PluginState {
    'fleet-comms': {
      thread: FleetThread | 'loading' | null
      isBandHidden: boolean
      openRows: string[]
      seenMail: string | null
      goalShown: boolean
      anchor: string | null
      link: Link
      status: FleetStatusState
      statusShown: boolean
      statusWhole: boolean
      chosenSlug: string | null
      openTasks: string[]
      closedShown: boolean
      stopArmed: string | null
      dismissed: string | null
      goalArmed: string | null
      goalCleared: boolean
    }
  }
}
