import { CHAMFER, HOP_R, TRACE_KINDS, type Point, type TraceKind } from './types';

export function cleanPoints(points: readonly Point[]): Point[] {
	const out: Point[] = [];
	for (const p of points) {
		const last = out[out.length - 1];
		if (!last || Math.abs(p[0] - last[0]) > 0.01 || Math.abs(p[1] - last[1]) > 0.01) {
			out.push(p);
		}
	}
	if (out.length === 0) return out;
	const keep: Point[] = [out[0]!];
	for (let i = 1; i < out.length - 1; i++) {
		const a0 = out[i - 1]!;
		const b = out[i]!;
		const c = out[i + 1]!;
		const sameX = Math.abs(a0[0] - b[0]) < 0.01 && Math.abs(b[0] - c[0]) < 0.01;
		const sameY = Math.abs(a0[1] - b[1]) < 0.01 && Math.abs(b[1] - c[1]) < 0.01;
		if (sameX || sameY) continue;
		keep.push(b);
	}
	if (out.length > 1) keep.push(out[out.length - 1]!);
	return keep;
}

function dedupeConsecutive(points: readonly Point[]): Point[] {
	const out: Point[] = [];
	points.forEach((p, i) => {
		const prev = points[i - 1];
		if (i === 0 || !prev || Math.abs(p[0] - prev[0]) > 0.01 || Math.abs(p[1] - prev[1]) > 0.01) {
			out.push(p);
		}
	});
	return out;
}

const f1 = (n: number): string => n.toFixed(1);

export function tracePath(
	points: readonly Point[],
	chamfer = CHAMFER,
	hops: readonly Point[] = []
): string {
	const pts = dedupeConsecutive(points);
	if (pts.length < 2) return '';

	const cuts = new Array(pts.length).fill(0);
	for (let i = 1; i < pts.length - 1; i++) {
		const lIn = Math.hypot(pts[i]![0] - pts[i - 1]![0], pts[i]![1] - pts[i - 1]![1]);
		const lOut = Math.hypot(pts[i + 1]![0] - pts[i]![0], pts[i + 1]![1] - pts[i]![1]);
		let cc = Math.min(chamfer, lIn / 2, lOut / 2);
		if (cc < 1) cc = 0;
		cuts[i] = cc;
	}

	let d = '';
	for (let i = 0; i < pts.length - 1; i++) {
		const [x0, y0] = pts[i]!;
		const [x1, y1] = pts[i + 1]!;
		const L = Math.hypot(x1 - x0, y1 - y0) || 1;
		const ux = (x1 - x0) / L;
		const uy = (y1 - y0) / L;
		const sx = x0 + ux * cuts[i];
		const sy = y0 + uy * cuts[i];
		const ex = x1 - ux * cuts[i + 1];
		const ey = y1 - uy * cuts[i + 1];
		d += i === 0 ? `M ${f1(sx)} ${f1(sy)}` : ` L ${f1(sx)} ${f1(sy)}`;
		if (Math.abs(x1 - x0) < 0.01) {
			const [ylo, yhi] = [Math.min(y0, y1), Math.max(y0, y1)];
			const hs = hops
				.filter((h) => Math.abs(h[0] - x0) < 0.01 && ylo + HOP_R < h[1] && h[1] < yhi - HOP_R)
				.slice()
				.sort((a, b) => (y1 < y0 ? b[1] - a[1] : a[1] - b[1]));
			for (const [hx, hy] of hs) {
				const sweep = uy > 0 ? 1 : 0;
				d += ` L ${f1(hx)} ${f1(hy - uy * HOP_R)} A ${HOP_R} ${HOP_R} 0 0 ${sweep} ${f1(hx)} ${f1(hy + uy * HOP_R)}`;
			}
		}
		d += ` L ${f1(ex)} ${f1(ey)}`;
	}
	return d;
}

export function pathLength(points: readonly Point[], chamfer = CHAMFER, hopCount = 0): number {
	const pts = cleanPoints(points);
	let total = 0;
	for (let i = 0; i < pts.length - 1; i++) {
		total += Math.hypot(pts[i + 1]![0] - pts[i]![0], pts[i + 1]![1] - pts[i]![1]);
	}
	for (let i = 1; i < pts.length - 1; i++) {
		const lIn = Math.hypot(pts[i]![0] - pts[i - 1]![0], pts[i]![1] - pts[i - 1]![1]);
		const lOut = Math.hypot(pts[i + 1]![0] - pts[i]![0], pts[i + 1]![1] - pts[i]![1]);
		const cc = Math.min(chamfer, lIn / 2, lOut / 2);
		if (cc >= 1) total -= 2 * cc - cc * Math.SQRT2;
	}
	return total + hopCount * (Math.PI * 4 - 8);
}

export function dashArray(kind: TraceKind): string {
	return TRACE_KINDS[kind].pattern;
}
