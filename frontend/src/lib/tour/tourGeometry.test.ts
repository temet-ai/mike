import { describe, it, expect } from 'vitest';
import { centeredPosition, placePopover, sameRect, spotlightRect } from './tourGeometry';

const VP = { width: 1200, height: 800 };
const POP = { width: 320, height: 180 };

describe('spotlightRect', () => {
	it('grows the target by the padding on every side', () => {
		expect(spotlightRect({ top: 100, left: 50, width: 200, height: 40 }, 8)).toEqual({
			top: 92,
			left: 42,
			width: 216,
			height: 56
		});
	});
});

describe('sameRect', () => {
	it('ignores sub-pixel jitter and treats null only as equal to null', () => {
		const a = { top: 1, left: 1, width: 10, height: 10 };
		expect(sameRect(a, { top: 1.2, left: 1, width: 10, height: 10.3 })).toBe(true);
		expect(sameRect(a, { top: 3, left: 1, width: 10, height: 10 })).toBe(false);
		expect(sameRect(null, null)).toBe(true);
		expect(sameRect(a, null)).toBe(false);
	});
});

describe('placePopover', () => {
	it('places below a target near the top, centred, with the arrow on the target centre', () => {
		const pos = placePopover({ top: 40, left: 400, width: 400, height: 60 }, POP, VP);
		expect(pos.side).toBe('bottom');
		expect(pos.top).toBe(40 + 60 + 14);
		expect(pos.left).toBe(600 - 160);
		expect(pos.arrow).toEqual({ x: 600, y: pos.top });
	});

	it('flips to the top when there is no room below', () => {
		const pos = placePopover({ top: 650, left: 400, width: 400, height: 60 }, POP, VP);
		expect(pos.side).toBe('top');
		expect(pos.top).toBe(650 - 14 - 180);
		expect(pos.arrow?.y).toBe(650 - 14);
	});

	it('honours a preferred side when it fits', () => {
		const pos = placePopover({ top: 300, left: 100, width: 200, height: 100 }, POP, VP, 'right');
		expect(pos.side).toBe('right');
		expect(pos.left).toBe(100 + 200 + 14);
		expect(pos.arrow?.x).toBe(pos.left);
	});

	it('clamps into the viewport and keeps the arrow inside the popover edge', () => {
		const pos = placePopover({ top: 40, left: 0, width: 40, height: 40 }, POP, VP);
		expect(pos.left).toBe(12);
		expect(pos.arrow?.x).toBe(12 + 18);
	});

	it('falls back to an arrowless overlay at the bottom of the viewport for a target that fills the screen', () => {
		const pos = placePopover({ top: 0, left: 0, width: 1200, height: 800 }, POP, VP);
		expect(pos.side).toBe('overlay');
		expect(pos.arrow).toBeNull();
		expect(pos.top).toBe(800 - 12 - 180);
	});
});

describe('centeredPosition', () => {
	it('centres the popover with no arrow', () => {
		expect(centeredPosition(POP, VP)).toEqual({
			side: 'overlay',
			top: 310,
			left: 440,
			arrow: null
		});
	});
});
