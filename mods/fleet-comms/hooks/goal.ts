import type { FleetPanel } from "../types";
import { isLive } from "./status";

export const GOAL_MAX_CHARS = 4000;

const CONTROL_ROLES = ["coordinator", "acceptance", "observer"];

export function isControlSeat(panel: FleetPanel | null, session: string): boolean {
  if (panel === null || !panel.isOwn || session === "") return false;
  const role = panel.seats.find((s) => s.session === session)?.role ?? "";
  return CONTROL_ROLES.includes(role);
}

export function ownGoal(panel: FleetPanel | null, session: string): string | null {
  if (panel === null || !panel.isOwn || !isLive(panel) || session === "") return null;
  const goal = panel.seats.find((s) => s.session === session)?.goal ?? null;
  if (goal === null || goal.trim() === "" || goal.length > GOAL_MAX_CHARS) return null;
  return goal;
}
