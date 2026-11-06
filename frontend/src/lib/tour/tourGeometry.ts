export interface Rect {
	top: number;
	left: number;
	width: number;
	height: number;
}

export interface Size {
	width: number;
	height: number;
}

export type Side = 'top' | 'bottom' | 'left' | 'right';
export type Placement = Side | 'auto';

export interface PopoverPosition {
	/** `overlay` means no side had room: the popover sits at the bottom of the viewport with no arrow. */
	side: Side | 'overlay';
	top: number;
	left: number;
	/** Viewport coordinates of the arrow's centre, or null when there is no arrow. */
	arrow: { x: number; y: number } | null;
}

export interface PlaceOptions {
	gap?: number;
	margin?: number;
	arrowInset?: number;
}

const AUTO_ORDER: Side[] = ['bottom', 'top', 'right', 'left'];

function clamp(value: number, min: number, max: number): number {
	if (max < min) return min;
	return Math.min(Math.max(value, min), max);
}

export function spotlightRect(target: Rect, padding: number): Rect {
	return {
		top: target.top - padding,
		left: target.left - padding,
		width: target.width + padding * 2,
		height: target.height + padding * 2
	};
}

export function sameRect(a: Rect | null, b: Rect | null): boolean {
	if (a === null || b === null) return a === b;
	return (
		Math.abs(a.top - b.top) < 0.5 &&
		Math.abs(a.left - b.left) < 0.5 &&
		Math.abs(a.width - b.width) < 0.5 &&
		Math.abs(a.height - b.height) < 0.5
	);
}

function fits(side: Side, spot: Rect, pop: Size, vp: Size, gap: number, margin: number): boolean {
	switch (side) {
		case 'bottom':
			return spot.top + spot.height + gap + pop.height <= vp.height - margin;
		case 'top':
			return spot.top - gap - pop.height >= margin;
		case 'right':
			return spot.left + spot.width + gap + pop.width <= vp.width - margin;
		case 'left':
			return spot.left - gap - pop.width >= margin;
	}
}

function sideOrder(preferred: Placement): Side[] {
	if (preferred === 'auto') return AUTO_ORDER;
	const opposite: Record<Side, Side> = {
		top: 'bottom',
		bottom: 'top',
		left: 'right',
		right: 'left'
	};
	const rest = AUTO_ORDER.filter((s) => s !== preferred && s !== opposite[preferred]);
	return [preferred, opposite[preferred], ...rest];
}

function placeOnSide(
	side: Side,
	spot: Rect,
	pop: Size,
	vp: Size,
	opts: Required<PlaceOptions>
): PopoverPosition {
	const { gap, margin, arrowInset } = opts;
	const vertical = side === 'top' || side === 'bottom';
	if (vertical) {
		const centerX = spot.left + spot.width / 2;
		const left = clamp(centerX - pop.width / 2, margin, vp.width - margin - pop.width);
		const top = side === 'bottom' ? spot.top + spot.height + gap : spot.top - gap - pop.height;
		const x = clamp(centerX, left + arrowInset, left + pop.width - arrowInset);
		const y = side === 'bottom' ? top : top + pop.height;
		return { side, top, left, arrow: { x, y } };
	}
	const centerY = spot.top + spot.height / 2;
	const top = clamp(centerY - pop.height / 2, margin, vp.height - margin - pop.height);
	const left = side === 'right' ? spot.left + spot.width + gap : spot.left - gap - pop.width;
	const y = clamp(centerY, top + arrowInset, top + pop.height - arrowInset);
	const x = side === 'right' ? left : left + pop.width;
	return { side, top, left, arrow: { x, y } };
}

export function centeredPosition(pop: Size, vp: Size): PopoverPosition {
	return {
		side: 'overlay',
		top: Math.max(0, (vp.height - pop.height) / 2),
		left: Math.max(0, (vp.width - pop.width) / 2),
		arrow: null
	};
}

export function placePopover(
	spot: Rect,
	pop: Size,
	vp: Size,
	preferred: Placement = 'auto',
	options: PlaceOptions = {}
): PopoverPosition {
	const opts: Required<PlaceOptions> = {
		gap: options.gap ?? 14,
		margin: options.margin ?? 12,
		arrowInset: options.arrowInset ?? 18
	};
	const side = sideOrder(preferred).find((s) => fits(s, spot, pop, vp, opts.gap, opts.margin));
	if (side) return placeOnSide(side, spot, pop, vp, opts);
	return {
		side: 'overlay',
		top: Math.max(opts.margin, vp.height - opts.margin - pop.height),
		left: clamp(
			spot.left + spot.width / 2 - pop.width / 2,
			opts.margin,
			vp.width - opts.margin - pop.width
		),
		arrow: null
	};
}
