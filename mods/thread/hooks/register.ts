import type { EngineInterface, Register } from 'claude-code'

import { register as registerAssist, startAssist } from './assist'
import { register as registerBoard, startBoard } from './board'
import { register as registerCard } from './card'
import { BRIEF_STATUSES, isListing, tetherLoaded, threadsBrief } from './brief'

const INSPECT = 'http://127.0.0.1:9989'

let briefedSession: string | undefined

async function underTether($: EngineInterface): Promise<boolean> {
  try {
    return tetherLoaded(await $.env.get('CLAUDE_CODE_PLUGIN_DIRS'))
  } catch {
    return false
  }
}

async function briefFor($: EngineInterface): Promise<string | null> {
  if (await underTether($)) return null
  const root = await $.session.root()
  const query = `project=${encodeURIComponent(root)}&status=${encodeURIComponent(BRIEF_STATUSES)}&limit=100`
  try {
    const res = await $.http.fetch(`${INSPECT}/api/work-threads?${query}`)
    if (!res.ok) return null
    const body: unknown = JSON.parse(res.text)
    return isListing(body) ? threadsBrief(body) : null
  } catch {
    return null
  }
}

export const register: Register = on => {
  registerAssist(on)
  registerBoard(on)
  registerCard(on)
  on('session.start', async ($, e, next) => {
    briefedSession = undefined
    startAssist($)
    await startBoard($)
    return next(e)
  })

  on('session.compact', async (_$, e, next) => {
    const result = await next(e)
    if (result.messages && !e.agentId && e.trigger !== 'precompute') briefedSession = undefined
    return result
  })

  on('prompt.submit', async ($, e, next) => {
    if (e.command !== undefined || e.isMeta === true || e.text.trim() === '') return next(e)
    const session = await $.session.id()
    if (session === briefedSession) return next(e)
    briefedSession = session
    const brief = await briefFor($)
    if (brief === null) return next(e)
    return next({ ...e, context: [...(e.context ?? []), brief] })
  })
}
