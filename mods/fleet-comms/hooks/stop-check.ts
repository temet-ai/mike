import type { EngineInterface } from "claude-code";

export type StopVerdict = { decision: "let_through" | "push_back"; reason: string };

async function post(
  $: EngineInterface,
  url: string,
  body: Record<string, unknown>,
): Promise<StopVerdict | null> {
  try {
    const res = await $.http.fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    if (!res.ok) return null;
    const parsed = JSON.parse(res.text) as { decision?: string; reason?: string };
    if (parsed.decision === "let_through" || parsed.decision === "push_back") {
      return { decision: parsed.decision, reason: String(parsed.reason ?? "") };
    }
    return null;
  } catch {
    return null;
  }
}

function stopFactsUrl(inspect: string, fleet: string, session: string): string {
  return `${inspect}/api/fleets/${fleet}/seats/${session}/stop-facts`;
}

export async function probeStopCheck(
  $: EngineInterface,
  inspect: string,
  fleet: string,
  session: string,
): Promise<boolean> {
  return (await post($, stopFactsUrl(inspect, fleet, session), { probe: true })) !== null;
}

export async function askStopCheck(
  $: EngineInterface,
  inspect: string,
  fleet: string,
  session: string,
  finalMessage: string | undefined,
  stopHookActive: boolean,
): Promise<StopVerdict | null> {
  return post($, stopFactsUrl(inspect, fleet, session), {
    final_message: finalMessage ?? null,
    stop_hook_active: stopHookActive,
  });
}
