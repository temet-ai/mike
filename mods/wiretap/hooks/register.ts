import type { EngineInterface, Register } from 'claude-code'

const OUT = '/mnt/data/projects/temet/temet-proxy/artifacts/2026-10-02/mods-vs-mike/wiretap'

type Row = { at: number; event: string; [k: string]: unknown }
const rows: Row[] = []
let file = ''

async function note($: EngineInterface, event: string, data: Record<string, unknown>) {
  if (file === '') file = `${OUT}/${await $.session.id()}.json`
  rows.push({ at: await $.clock.now(), event, ...data })
  await $.fs.write(file, JSON.stringify(rows, null, 1))
}

async function sha(text: string) {
  const d = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text))
  return [...new Uint8Array(d)].slice(0, 6).map(b => b.toString(16).padStart(2, '0')).join('')
}

export const register: Register = on => {
  on('session.start', async ($, e, next) => {
    await note($, 'session.start', { cwd: e.cwd, surface: e.surface, isInteractive: e.isInteractive, model: await $.session.model() })
    return next(e)
  })

  on('prompt.submit', async ($, e, next) => {
    await note($, 'prompt.submit', { text: e.text })
    return next(e)
  })

  on('prompt.compose', async ($, e, next) => {
    const out = await next(e)
    const texts = out.sections.map(s => ('text' in s ? String((s as { text: unknown }).text) : JSON.stringify(s)))
    await note($, 'prompt.compose', {
      model: e.model,
      sections: out.sections.length,
      chars: texts.reduce((n, t) => n + t.length, 0),
      hash: await sha(texts.join('\n')),
    })
    return out
  })

  on('turn.start', async ($, e, next) => {
    await note($, 'turn.start', { turnId: e.turnId })
    return next(e)
  })

  on('turn.step', async function* ($, e, next) {
    const result = yield* next(e)
    await note($, 'turn.step', {
      turnId: e.turnId,
      index: e.index,
      model: e.model,
      effort: e.effort ?? null,
      messageCount: e.messageCount,
      agentId: e.agentId ?? null,
      stopReason: result.stopReason,
      usage: result.usage,
      answerChars: result.answer.length,
      toolUses: result.toolUses.map(t => t.name),
    })
    return result
  })

  on('turn.complete', async ($, e, next) => {
    await note($, 'turn.complete', { turnId: e.turnId, agentId: e.agentId ?? null, durationMs: e.durationMs, usage: e.usage, isAborted: e.isAborted })
    return next(e)
  })

  on('tool.call', async ($, e, next) => {
    const result = await next(e)
    await note($, 'tool.call', { tool: e.tool, agentId: e.agentId ?? null, isError: 'isError' in result ? result.isError ?? false : false, denied: 'deny' in result })
    return result
  })

  on('session.append', async ($, e, next) => {
    await note($, 'session.append', { door: e.door, origin: e.origin, uuid: e.uuid, agentId: e.agentId ?? null })
    return next(e)
  })

  on('session.compact', async ($, e, next) => {
    await note($, 'session.compact', {})
    return next(e)
  })
}
