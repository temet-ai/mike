import { beforeEach, describe, expect, it } from 'vitest';
import { createLocalStorageMock } from '$lib/test-support/localStorage-mock';

const { storage, localStorageMock } = createLocalStorageMock();

Object.defineProperty(globalThis, 'localStorage', {
	value: localStorageMock,
	writable: true
});

import {
	clampColumnForContainer,
	clampLayoutValue,
	defaultAssistantOpen,
	defaultNavOpen,
	keyboardResizeDelta,
	loadWorkbenchLayout,
	occupiedSideWidth,
	PROMPT_MIN_WIDTH,
	saveWorkbenchLayout,
	WORKBENCH_BREAKPOINT_ALL_OPEN,
	WORKBENCH_BREAKPOINT_ASSISTANT,
	WORKBENCH_DIVIDER_WIDTH,
	WORKBENCH_LAYOUT_DEFAULTS,
	WORKBENCH_LAYOUT_STORAGE_KEY,
	WORKBENCH_RAIL_WIDTH,
	type WorkbenchLayout,
	type WorkbenchOpenState
} from './workbenchLayout';

const ALL_OPEN: WorkbenchOpenState = { nav: true, edits: true, assistant: true };

beforeEach(() => {
	storage.clear();
});

describe('clampLayoutValue', () => {
	it('passes through an in-bounds value (rounded)', () => {
		expect(clampLayoutValue('navWidth', 250.4)).toBe(250);
	});

	it('clamps below the minimum', () => {
		expect(clampLayoutValue('navWidth', 10)).toBe(180);
	});

	it('clamps above the maximum', () => {
		expect(clampLayoutValue('assistantWidth', 9999)).toBe(680);
	});

	it('falls back to the default for non-finite input', () => {
		expect(clampLayoutValue('editsWidth', NaN)).toBe(WORKBENCH_LAYOUT_DEFAULTS.editsWidth);
		expect(clampLayoutValue('editsWidth', Infinity)).toBe(WORKBENCH_LAYOUT_DEFAULTS.editsWidth);
	});
});

describe('loadWorkbenchLayout', () => {
	it('returns the defaults when nothing is stored', () => {
		expect(loadWorkbenchLayout()).toEqual(WORKBENCH_LAYOUT_DEFAULTS);
	});

	it('returns the defaults when the stored value is malformed JSON', () => {
		localStorage.setItem(WORKBENCH_LAYOUT_STORAGE_KEY, '{not json');
		expect(loadWorkbenchLayout()).toEqual(WORKBENCH_LAYOUT_DEFAULTS);
	});

	it('returns the defaults when the stored value is not an object', () => {
		localStorage.setItem(WORKBENCH_LAYOUT_STORAGE_KEY, '[1,2,3]');
		expect(loadWorkbenchLayout()).toEqual(WORKBENCH_LAYOUT_DEFAULTS);
		localStorage.setItem(WORKBENCH_LAYOUT_STORAGE_KEY, '"a string"');
		expect(loadWorkbenchLayout()).toEqual(WORKBENCH_LAYOUT_DEFAULTS);
	});

	it('round-trips a value saved by saveWorkbenchLayout', () => {
		const layout = { navWidth: 300, editsWidth: 400, assistantWidth: 500 };
		saveWorkbenchLayout(layout);
		expect(loadWorkbenchLayout()).toEqual(layout);
	});

	it('clamps an out-of-range stored value rather than trusting it verbatim', () => {
		localStorage.setItem(
			WORKBENCH_LAYOUT_STORAGE_KEY,
			JSON.stringify({ navWidth: 5, editsWidth: 5000, assistantWidth: 50 })
		);
		expect(loadWorkbenchLayout()).toEqual({ navWidth: 180, editsWidth: 640, assistantWidth: 320 });
	});

	it('fills in missing keys from defaults', () => {
		localStorage.setItem(WORKBENCH_LAYOUT_STORAGE_KEY, JSON.stringify({ navWidth: 250 }));
		expect(loadWorkbenchLayout()).toEqual({
			navWidth: 250,
			editsWidth: WORKBENCH_LAYOUT_DEFAULTS.editsWidth,
			assistantWidth: WORKBENCH_LAYOUT_DEFAULTS.assistantWidth
		});
	});

	it('ignores non-numeric fields and uses the default instead', () => {
		localStorage.setItem(
			WORKBENCH_LAYOUT_STORAGE_KEY,
			JSON.stringify({ navWidth: 'wide', editsWidth: 400, assistantWidth: 500 })
		);
		expect(loadWorkbenchLayout()).toEqual({
			navWidth: WORKBENCH_LAYOUT_DEFAULTS.navWidth,
			editsWidth: 400,
			assistantWidth: 500
		});
	});

	it('ignores a legacy sideWidth/editsHeight blob from the previous two-column layout', () => {
		localStorage.setItem(
			WORKBENCH_LAYOUT_STORAGE_KEY,
			JSON.stringify({ navWidth: 230, sideWidth: 460, editsHeight: 320 })
		);
		expect(loadWorkbenchLayout()).toEqual({
			navWidth: 230,
			editsWidth: WORKBENCH_LAYOUT_DEFAULTS.editsWidth,
			assistantWidth: WORKBENCH_LAYOUT_DEFAULTS.assistantWidth
		});
	});
});

describe('keyboardResizeDelta', () => {
	it('is positive for ArrowRight and ArrowDown', () => {
		expect(keyboardResizeDelta('ArrowRight')).toBeGreaterThan(0);
		expect(keyboardResizeDelta('ArrowDown')).toBeGreaterThan(0);
	});

	it('is negative for ArrowLeft and ArrowUp', () => {
		expect(keyboardResizeDelta('ArrowLeft')).toBeLessThan(0);
		expect(keyboardResizeDelta('ArrowUp')).toBeLessThan(0);
	});

	it('is zero for any other key', () => {
		expect(keyboardResizeDelta('Enter')).toBe(0);
		expect(keyboardResizeDelta('a')).toBe(0);
	});

	it('ArrowRight and ArrowLeft are exact opposites (symmetric step)', () => {
		expect(keyboardResizeDelta('ArrowRight')).toBe(-keyboardResizeDelta('ArrowLeft'));
	});
});

describe('defaultNavOpen / defaultAssistantOpen', () => {
	it('opens the outline only at or above the all-open breakpoint', () => {
		expect(defaultNavOpen(WORKBENCH_BREAKPOINT_ALL_OPEN)).toBe(true);
		expect(defaultNavOpen(WORKBENCH_BREAKPOINT_ALL_OPEN + 1)).toBe(true);
		expect(defaultNavOpen(WORKBENCH_BREAKPOINT_ALL_OPEN - 1)).toBe(false);
	});

	it('opens the assistant only at or above its breakpoint', () => {
		expect(defaultAssistantOpen(WORKBENCH_BREAKPOINT_ASSISTANT)).toBe(true);
		expect(defaultAssistantOpen(WORKBENCH_BREAKPOINT_ASSISTANT + 1)).toBe(true);
		expect(defaultAssistantOpen(WORKBENCH_BREAKPOINT_ASSISTANT - 1)).toBe(false);
	});

	it('a mid-range viewport (1440) opens the assistant but not the outline', () => {
		expect(defaultNavOpen(1440)).toBe(false);
		expect(defaultAssistantOpen(1440)).toBe(true);
	});

	it('a narrow viewport (1200) opens neither', () => {
		expect(defaultNavOpen(1200)).toBe(false);
		expect(defaultAssistantOpen(1200)).toBe(false);
	});
});

describe('occupiedSideWidth', () => {
	it('sums all three open columns plus one divider each', () => {
		const layout: WorkbenchLayout = { navWidth: 220, editsWidth: 380, assistantWidth: 420 };
		expect(occupiedSideWidth(layout, ALL_OPEN)).toBe(
			220 + 380 + 420 + WORKBENCH_DIVIDER_WIDTH * 3
		);
	});

	it('uses the rail width and no divider for a collapsed column', () => {
		const layout: WorkbenchLayout = { navWidth: 220, editsWidth: 380, assistantWidth: 420 };
		const open: WorkbenchOpenState = { nav: false, edits: true, assistant: true };
		expect(occupiedSideWidth(layout, open)).toBe(
			WORKBENCH_RAIL_WIDTH + 380 + 420 + WORKBENCH_DIVIDER_WIDTH * 2
		);
	});

	it('is just three rails when everything is collapsed', () => {
		const layout: WorkbenchLayout = { navWidth: 220, editsWidth: 380, assistantWidth: 420 };
		const open: WorkbenchOpenState = { nav: false, edits: false, assistant: false };
		expect(occupiedSideWidth(layout, open)).toBe(WORKBENCH_RAIL_WIDTH * 3);
	});
});

describe('clampColumnForContainer', () => {
	const layout: WorkbenchLayout = { navWidth: 220, editsWidth: 380, assistantWidth: 420 };

	it('falls back to the column bounds alone when the container is not yet measured', () => {
		expect(clampColumnForContainer('navWidth', 300, layout, ALL_OPEN, 0)).toBe(300);
		expect(clampColumnForContainer('navWidth', 9999, layout, ALL_OPEN, NaN)).toBe(420);
	});

	it('allows growth that still leaves room for PROMPT_MIN_WIDTH', () => {
		// container 1920, others (edits 380 + assistant 420 + 2 dividers) = 814, so navWidth can
		// grow up to 1920 - 520 - 814 - 7(its own divider) = 579, clamped to its own max (420).
		const containerWidth = 1920;
		expect(clampColumnForContainer('navWidth', 400, layout, ALL_OPEN, containerWidth)).toBe(400);
	});

	it('clamps growth that would push the prompt column below PROMPT_MIN_WIDTH', () => {
		// A narrow container where growing navWidth to 400 would leave the prompt column under
		// its 520px floor: others (380 + 420 + 2 dividers = 814) + prompt floor (520) + this
		// column's own divider (7) = 1341, so at containerWidth 1300 there is no room left over
		// the column's own minimum.
		const containerWidth = 1300;
		const result = clampColumnForContainer('navWidth', 400, layout, ALL_OPEN, containerWidth);
		expect(result).toBe(180); // the column's own minimum, since roomForThisColumn is negative
	});

	it('a collapsed neighbour frees up room (rail width, no divider) for the column being resized', () => {
		const openWithEditsCollapsed: WorkbenchOpenState = { nav: true, edits: false, assistant: true };
		const containerWidth = 1350;
		const wide = clampColumnForContainer(
			'assistantWidth',
			670,
			layout,
			openWithEditsCollapsed,
			containerWidth
		);
		const narrow = clampColumnForContainer('assistantWidth', 670, layout, ALL_OPEN, containerWidth);
		expect(wide).toBeGreaterThan(narrow);
	});

	it('never returns less than the column minimum even under extreme container pressure', () => {
		const result = clampColumnForContainer('assistantWidth', 600, layout, ALL_OPEN, 600);
		expect(result).toBe(320);
	});

	it('respects PROMPT_MIN_WIDTH as the named floor in a realistic 1280 viewport', () => {
		// At the assistant breakpoint (1280) with the outline collapsed to a rail, growing the
		// edits column is clamped well below its own 640 max so the prompt column keeps its
		// 520px floor rather than being squeezed by a greedy drag.
		const open: WorkbenchOpenState = { nav: false, edits: true, assistant: true };
		const containerWidth = WORKBENCH_BREAKPOINT_ASSISTANT;
		const result = clampColumnForContainer('editsWidth', 900, layout, open, containerWidth);
		expect(result).toBe(290);
		expect(
			containerWidth - occupiedSideWidth({ ...layout, editsWidth: result }, open)
		).toBeGreaterThanOrEqual(PROMPT_MIN_WIDTH);
	});
});
