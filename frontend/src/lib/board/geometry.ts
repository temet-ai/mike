import type { ChipRect, PadSide, Point, Route } from './types';

export function chipPads(rect: ChipRect): Record<PadSide, Point> {
	const { x, y, w, h } = rect;
	return {
		right: [x + w, y + h / 2],
		righthi: [x + w, y + 8],
		rightlo: [x + w, y + h - 8],
		left: [x, y + h / 2],
		lefthi: [x, y + 8],
		leftlo: [x, y + h - 8],
		top: [x + w - 12, y],
		topl: [x + 12, y],
		topc: [x + w / 2, y],
		bottom: [x + 12, y + h],
		bottomc: [x + w / 2, y + h],
		bottomr: [x + w - 12, y + h]
	};
}

export function segments(points: readonly Point[]): Array<readonly [Point, Point]> {
	const out: Array<readonly [Point, Point]> = [];
	for (let i = 0; i < points.length - 1; i++) {
		out.push([points[i]!, points[i + 1]!]);
	}
	return out;
}

const isHorizontal = (seg: readonly [Point, Point]): boolean =>
	Math.abs(seg[0][1] - seg[1][1]) < 0.01;

export function crosses(h: readonly [Point, Point], v: readonly [Point, Point]): boolean {
	const hIsH = isHorizontal(h);
	const vIsH = isHorizontal(v);
	if (hIsH === vIsH) return false;
	const [hs, vs] = hIsH ? [h, v] : [v, h];
	const [[ax, ay], [bx]] = hs;
	const [[cx, cy], [, dy]] = vs;
	const [x0, x1] = [Math.min(ax, bx), Math.max(ax, bx)];
	const [y0, y1] = [Math.min(cy, dy), Math.max(cy, dy)];
	return x0 + 0.5 < cx && cx < x1 - 0.5 && y0 + 0.5 < ay && ay < y1 - 0.5;
}

export function crossPoint(h: readonly [Point, Point], v: readonly [Point, Point]): Point | null {
	const hIsH = isHorizontal(h);
	const vIsH = isHorizontal(v);
	if (hIsH === vIsH) return null;
	const [hs, vs] = hIsH ? [h, v] : [v, h];
	const [[ax, ay], [bx]] = hs;
	const [[cx, cy], [, dy]] = vs;
	const [x0, x1] = [Math.min(ax, bx), Math.max(ax, bx)];
	const [y0, y1] = [Math.min(cy, dy), Math.max(cy, dy)];
	if (x0 + 0.5 < cx && cx < x1 - 0.5 && y0 + 0.5 < ay && ay < y1 - 0.5) {
		return [cx, ay];
	}
	return null;
}

export function crossings(routes: readonly Route[]): Array<[string, string, Point]> {
	const out: Array<[string, string, Point]> = [];
	const segsByRoute = routes.map((r) => segments(r.points));
	for (let i = 0; i < routes.length; i++) {
		for (let j = i + 1; j < routes.length; j++) {
			for (const s1 of segsByRoute[i]!) {
				for (const s2 of segsByRoute[j]!) {
					const pt = crossPoint(s1, s2);
					if (pt) out.push([routes[i]!.key, routes[j]!.key, pt]);
				}
			}
		}
	}
	return out;
}

export function entersRect(seg: readonly [Point, Point], rect: ChipRect): boolean {
	const [[ax, ay], [bx, by]] = seg;
	const { x, y, w, h } = rect;
	const left = x + 0.5;
	const right = x + w - 0.5;
	const top = y + 0.5;
	const bottom = y + h - 0.5;
	const steps = 64;
	for (let i = 0; i <= steps; i++) {
		const t = i / steps;
		const px = ax + (bx - ax) * t;
		const py = ay + (by - ay) * t;
		if (px > left && px < right && py > top && py < bottom) return true;
	}
	return false;
}

export function tooClose(
	routes: readonly Route[],
	pitch: number
): Array<[string, string, readonly [Point, Point], readonly [Point, Point]]> {
	const out: Array<[string, string, readonly [Point, Point], readonly [Point, Point]]> = [];
	const segsByRoute = routes.map((r) => segments(r.points));
	for (let i = 0; i < routes.length; i++) {
		for (let j = i + 1; j < routes.length; j++) {
			for (const s1 of segsByRoute[i]!) {
				for (const s2 of segsByRoute[j]!) {
					const pair = tooCloseSegPair(s1, s2, pitch);
					if (pair) out.push([routes[i]!.key, routes[j]!.key, s1, s2]);
				}
			}
		}
	}
	return out;
}

function tooCloseSegPair(
	s1: readonly [Point, Point],
	s2: readonly [Point, Point],
	pitch: number
): boolean {
	const h1 = isHorizontal(s1);
	const h2 = isHorizontal(s2);
	if (h1 !== h2) return false;
	if (h1) {
		const y1 = s1[0][1];
		const y2 = s2[0][1];
		if (Math.abs(y1 - y2) < 0.01) return false;
		if (Math.abs(y1 - y2) >= pitch - 0.01) return false;
		const [ax0, ax1] = [Math.min(s1[0][0], s1[1][0]), Math.max(s1[0][0], s1[1][0])];
		const [bx0, bx1] = [Math.min(s2[0][0], s2[1][0]), Math.max(s2[0][0], s2[1][0])];
		return ax0 < bx1 && bx0 < ax1;
	}
	const x1 = s1[0][0];
	const x2 = s2[0][0];
	if (Math.abs(x1 - x2) < 0.01) return false;
	if (Math.abs(x1 - x2) >= pitch - 0.01) return false;
	const [ay0, ay1] = [Math.min(s1[0][1], s1[1][1]), Math.max(s1[0][1], s1[1][1])];
	const [by0, by1] = [Math.min(s2[0][1], s2[1][1]), Math.max(s2[0][1], s2[1][1])];
	return ay0 < by1 && by0 < ay1;
}
