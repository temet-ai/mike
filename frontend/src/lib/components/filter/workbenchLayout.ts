/**
 * Pane-size and collapse-default logic for the filter workbench's four columns (Outline |
 * Prompt | Your edits | Prompt assistant). Pure functions so the drag/keyboard-resize and
 * responsive-default logic in `FilterWorkbench.svelte` stays testable without mounting a
 * component.
 */

export interface WorkbenchLayout {
	navWidth: number;
	editsWidth: number;
	assistantWidth: number;
}

export type ResizableLayoutKey = keyof WorkbenchLayout;

/** Which `WorkbenchOpenState` flag corresponds to each resizable layout key. */
const OPEN_STATE_KEY: Record<ResizableLayoutKey, keyof WorkbenchOpenState> = {
	navWidth: 'nav',
	editsWidth: 'edits',
	assistantWidth: 'assistant'
};

export interface WorkbenchOpenState {
	nav: boolean;
	edits: boolean;
	assistant: boolean;
}

export const WORKBENCH_LAYOUT_STORAGE_KEY = 'proxy-inspector:filter-workbench-layout';

export const WORKBENCH_LAYOUT_DEFAULTS: WorkbenchLayout = {
	navWidth: 220,
	editsWidth: 380,
	assistantWidth: 420
};

export const WORKBENCH_LAYOUT_BOUNDS: Record<ResizableLayoutKey, { min: number; max: number }> = {
	navWidth: { min: 180, max: 420 },
	editsWidth: { min: 280, max: 640 },
	assistantWidth: { min: 320, max: 680 }
};

/** Width of a collapsed column's labelled rail (spec 4.x: "collapse to a 36px labelled rail"). */
export const WORKBENCH_RAIL_WIDTH = 36;

/** Matches `WorkbenchDivider`'s own `.wb-divider--vertical { width: 7px; }`. */
export const WORKBENCH_DIVIDER_WIDTH = 7;

/** The prompt column never renders narrower than this, whatever the side columns are doing. */
export const PROMPT_MIN_WIDTH = 520;

/** Viewport floor for every column starting open. Below it, the outline starts collapsed. */
export const WORKBENCH_BREAKPOINT_ALL_OPEN = 1680;

/** Viewport floor for the assistant column starting open. */
export const WORKBENCH_BREAKPOINT_ASSISTANT = 1280;

export function clampLayoutValue(key: ResizableLayoutKey, value: number): number {
	const { min, max } = WORKBENCH_LAYOUT_BOUNDS[key];
	if (!Number.isFinite(value)) return WORKBENCH_LAYOUT_DEFAULTS[key];
	return Math.min(max, Math.max(min, Math.round(value)));
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export function loadWorkbenchLayout(): WorkbenchLayout {
	try {
		const raw = localStorage.getItem(WORKBENCH_LAYOUT_STORAGE_KEY);
		if (raw === null) return { ...WORKBENCH_LAYOUT_DEFAULTS };
		const parsed: unknown = JSON.parse(raw);
		if (!isPlainObject(parsed)) return { ...WORKBENCH_LAYOUT_DEFAULTS };
		const navWidth = typeof parsed.navWidth === 'number' ? parsed.navWidth : undefined;
		const editsWidth = typeof parsed.editsWidth === 'number' ? parsed.editsWidth : undefined;
		const assistantWidth =
			typeof parsed.assistantWidth === 'number' ? parsed.assistantWidth : undefined;
		return {
			navWidth: clampLayoutValue('navWidth', navWidth ?? WORKBENCH_LAYOUT_DEFAULTS.navWidth),
			editsWidth: clampLayoutValue(
				'editsWidth',
				editsWidth ?? WORKBENCH_LAYOUT_DEFAULTS.editsWidth
			),
			assistantWidth: clampLayoutValue(
				'assistantWidth',
				assistantWidth ?? WORKBENCH_LAYOUT_DEFAULTS.assistantWidth
			)
		};
	} catch {
		return { ...WORKBENCH_LAYOUT_DEFAULTS };
	}
}

export function saveWorkbenchLayout(layout: WorkbenchLayout): void {
	try {
		localStorage.setItem(WORKBENCH_LAYOUT_STORAGE_KEY, JSON.stringify(layout));
	} catch {
		// localStorage unavailable (private mode, storage full) — layout just stops persisting.
	}
}

const KEYBOARD_RESIZE_STEP = 16;

/**
 * The signed pixel delta a resize-handle keydown represents, BEFORE the divider's own
 * left/right orientation is applied. `ArrowRight`/`ArrowDown` are always "increase",
 * `ArrowLeft`/`ArrowUp` are always "decrease" — the caller multiplies by its own direction sign
 * (a divider whose resizable column sits to its LEFT, like the outline's, grows that column
 * when dragged right; one whose column sits to its RIGHT, like the edits/assistant dividers,
 * shrinks that column when dragged right instead).
 */
export function keyboardResizeDelta(key: string): number {
	if (key === 'ArrowRight' || key === 'ArrowDown') return KEYBOARD_RESIZE_STEP;
	if (key === 'ArrowLeft' || key === 'ArrowUp') return -KEYBOARD_RESIZE_STEP;
	return 0;
}

function columnFootprint(open: boolean, width: number): number {
	return open ? width : WORKBENCH_RAIL_WIDTH;
}

/**
 * Total width the three side columns (rail or full width) plus their dividers occupy right
 * now — NOT counting the prompt column. Used both to size `wb-workspace` bookkeeping and as
 * the basis for `clampColumnForContainer`'s prompt-floor math.
 */
export function occupiedSideWidth(layout: WorkbenchLayout, open: WorkbenchOpenState): number {
	const nav = columnFootprint(open.nav, layout.navWidth);
	const edits = columnFootprint(open.edits, layout.editsWidth);
	const assistant = columnFootprint(open.assistant, layout.assistantWidth);
	const dividers =
		(open.nav ? WORKBENCH_DIVIDER_WIDTH : 0) +
		(open.edits ? WORKBENCH_DIVIDER_WIDTH : 0) +
		(open.assistant ? WORKBENCH_DIVIDER_WIDTH : 0);
	return nav + edits + assistant + dividers;
}

function otherColumnsWidth(
	key: ResizableLayoutKey,
	layout: WorkbenchLayout,
	open: WorkbenchOpenState
): number {
	const keys: ResizableLayoutKey[] = ['navWidth', 'editsWidth', 'assistantWidth'];
	let total = 0;
	for (const k of keys) {
		if (k === key) continue;
		const openKey = OPEN_STATE_KEY[k];
		total += columnFootprint(open[openKey], layout[k]);
		if (open[openKey]) total += WORKBENCH_DIVIDER_WIDTH;
	}
	return total;
}

/**
 * Clamps a proposed width for one resizable column to its own [min, max] AND to whatever room
 * is left once the container width, the OTHER open columns, and `PROMPT_MIN_WIDTH` are
 * accounted for — so dragging a divider can never squeeze the prompt column below its floor
 * (spec: "Prompt column min width 520px — dividers clamp so it never goes below"). A column's
 * own minimum still wins if the container is too narrow to honour both floors at once.
 * `containerWidth <= 0` (not measured yet, e.g. before the first `bind:clientWidth` tick) falls
 * back to the column's own bounds only.
 */
export function clampColumnForContainer(
	key: ResizableLayoutKey,
	proposedValue: number,
	layout: WorkbenchLayout,
	open: WorkbenchOpenState,
	containerWidth: number
): number {
	const ownClamped = clampLayoutValue(key, proposedValue);
	if (!Number.isFinite(containerWidth) || containerWidth <= 0) return ownClamped;

	const openKey = OPEN_STATE_KEY[key];
	const others = otherColumnsWidth(key, layout, open);
	const dividerForThisColumn = open[openKey] ? WORKBENCH_DIVIDER_WIDTH : 0;
	const roomForThisColumn = containerWidth - PROMPT_MIN_WIDTH - others - dividerForThisColumn;

	const { min } = WORKBENCH_LAYOUT_BOUNDS[key];
	return Math.max(min, Math.min(ownClamped, roomForThisColumn));
}

/**
 * Responsive default for the outline column (spec: "viewport >= 1680px all open ... below that
 * the outline starts collapsed"). `width` is the workbench's OWN measured width — it fills
 * essentially the whole browser viewport on the real route, so the two are the same number in
 * practice, and measuring the component itself (rather than reading `window.innerWidth`
 * directly) is what makes this reproducible in a fixed-width Storybook wrapper. Applied once,
 * the first time that width is known, never re-applied on a later resize, so a manual toggle
 * mid-session is never overridden by the window changing size.
 */
export function defaultNavOpen(width: number): boolean {
	return width >= WORKBENCH_BREAKPOINT_ALL_OPEN;
}

/** Responsive default for the assistant column (spec: "below 1280px the assistant starts
 *  collapsed"). Same one-time-at-first-measurement semantics as `defaultNavOpen`. */
export function defaultAssistantOpen(width: number): boolean {
	return width >= WORKBENCH_BREAKPOINT_ASSISTANT;
}
