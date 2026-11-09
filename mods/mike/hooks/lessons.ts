export type Lesson = { id: number; kind: string; text: string; revision?: string }
export type LessonDelivery = {
  arm: 'off' | 'control' | 'mod'
  lessons: Lesson[]
  topic_catalog?: string | null
  memory_keeper?: string | null
  memory_keeper_hash?: string | null
}

export function keeperBlock(body: LessonDelivery, held: Set<string>): string | null {
  const text = body.memory_keeper
  const hash = body.memory_keeper_hash ?? text
  if (typeof text !== 'string' || text.trim() === '' || typeof hash !== 'string' || held.has(hash)) return null
  held.add(hash)
  return text
}

export const LESSONS_HEADING = 'Lessons recorded from earlier sessions in this project:'

export function lessonsBlock(lessons: readonly Lesson[]): string | null {
  if (lessons.length === 0) return null
  const lines = lessons.map(l => `[lesson:${l.id}] [${l.kind}] ${l.text.replace(/\s+/g, ' ').trim()}${l.revision ? ` [revision:${l.revision}]` : ''}`)
  return `<system-reminder>\n${LESSONS_HEADING}\nNew revisions replace earlier text with the same lesson id.\n${lines.join('\n')}\n</system-reminder>`
}

export function isDelivery(value: unknown): value is LessonDelivery {
  if (typeof value !== 'object' || value === null) return false
  const body = value as { arm?: unknown; lessons?: unknown }
  return typeof body.arm === 'string' && Array.isArray(body.lessons)
}

export function wantsLessons(e: { text: string; command?: unknown; isMeta?: true }): boolean {
  return e.command === undefined && e.isMeta !== true && e.text.trim() !== ''
}
