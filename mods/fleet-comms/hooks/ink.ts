export type InkKey =
  | "ink"
  | "dim"
  | "faint"
  | "green"
  | "yellow"
  | "red"
  | "blue"
  | "orange"
  | "purple";

export const INK: Record<InkKey, string> & {
  badgeText: string;
  ground: string;
  rowHover: string;
} = {
  ink: "rgb(192,202,245)",
  dim: "rgb(138,147,189)",
  faint: "rgb(86,95,137)",
  green: "rgb(158,206,106)",
  yellow: "rgb(224,175,104)",
  red: "rgb(247,118,142)",
  blue: "rgb(122,162,247)",
  orange: "rgb(255,158,100)",
  purple: "rgb(187,154,247)",
  badgeText: "rgb(26,27,38)",
  ground: "rgb(26,27,38)",
  rowHover: "rgb(41,46,66)",
};

const ROLE_COLOUR: Record<string, string> = {
  coordinator: "rgb(245,138,200)",
  observer: "rgb(247,118,142)",
  acceptance: "rgb(224,175,104)",
  "rust-developer": "rgb(255,158,100)",
  "frontend-developer": "rgb(158,206,106)",
  "db-dev": "rgb(122,162,247)",
  "ui-designer": "rgb(187,154,247)",
  "motion-designer": "rgb(125,207,255)",
  "unit-tester": "rgb(42,195,222)",
  unit: "rgb(185,242,124)",
};

export function roleColour(role: string): string | undefined {
  return Object.hasOwn(ROLE_COLOUR, role) ? ROLE_COLOUR[role] : undefined;
}

export function fleetStateColour(state: string): string {
  if (state === "running" || state === "done") return INK.green;
  if (state === "failed") return INK.red;
  return INK.yellow;
}
