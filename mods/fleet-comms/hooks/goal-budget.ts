import type { EngineInterface, PromptSubmitInput } from "claude-code";

export const DEFAULT_GOAL_CONTINUATIONS = 20;
export const MAX_GOAL_CONTINUATIONS = 200;

export type GoalBudget = {
  goal: string;
  continuations: number;
  exhausted: boolean;
  reported?: boolean;
};

export function continuationLimit(value: string | undefined): number {
  const limit = Number(value);
  return Number.isInteger(limit) && limit > 0 && limit <= MAX_GOAL_CONTINUATIONS
    ? limit
    : DEFAULT_GOAL_CONTINUATIONS;
}

export function freshGoalBudget(goal: string): GoalBudget {
  return { goal, continuations: 0, exhausted: false };
}

export function storedGoalBudget(value: unknown, goal: string): GoalBudget {
  if (value !== null && typeof value === "object") {
    const saved = value as Partial<GoalBudget>;
    if (saved.goal === goal && Number.isSafeInteger(saved.continuations)
      && saved.continuations! >= 0 && typeof saved.exhausted === "boolean") {
      return { goal, continuations: saved.continuations!, exhausted: saved.exhausted,
        ...(saved.reported ? { reported: true } : {}) };
    }
  }
  return freshGoalBudget(goal);
}

export function renewsGoalBudget(prompt: PromptSubmitInput): boolean {
  // Mike also pushes automated mail through Remote Control as user-shaped turns.
  // Only the engine-attested terminal gesture establishes operator renewal.
  return !prompt.isMeta && !prompt.command && prompt.text.trim() !== ""
    && prompt.origin.kind === "composer";
}

export function goalBudgetKey(session: string): string {
  return `goal-budget:${session}`;
}

export async function readGoalBudget(
  $: EngineInterface, session: string, goal: string,
): Promise<GoalBudget> {
  return storedGoalBudget(await $.store.get(goalBudgetKey(session)), goal);
}

export async function countGoalContinuation(
  $: EngineInterface, session: string, goal: string, active: boolean,
): Promise<GoalBudget> {
  const budget = await readGoalBudget($, session, goal);
  const limit = continuationLimit(await $.env.get("MIKE_FLEET_GOAL_MAX_CONTINUATIONS"));
  if (active && !budget.exhausted) budget.continuations += 1;
  budget.exhausted ||= budget.continuations >= limit;
  await $.store.set(goalBudgetKey(session), budget);
  return budget;
}

export function goalBudgetStopReason(budget: GoalBudget): string {
  return `Fleet goal paused after ${budget.continuations} automatic continuations. The task remains unfinished; its goal and last answer are retained. Mail can resume work. A new request typed at the terminal or a changed fleet goal renews the continuation budget.`;
}

export async function reportGoalPause($: EngineInterface, session: string, budget: GoalBudget): Promise<void> {
  if (budget.reported || !budget.exhausted) return;
  budget.reported = true;
  await $.store.set(goalBudgetKey(session), budget);
  try {
    await $.http.fetch(`http://localhost:9989/api/sessions/${session}/notes`, {
      method: "POST", headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ kind: "result", title: "Fleet continuation budget exhausted",
        tags: ["efficiency", "goal-budget"], data: { schema: "goal-budget-event-v1",
          event_id: `${session}:${Date.now()}`, continuations: budget.continuations } }),
    });
  } catch {
    // Telemetry cannot prevent the bounded session from stopping.
  }
}
