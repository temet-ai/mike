export interface TrendPoint {
	at: string;
	value: number;
}

export type TrendDirection = 'lower' | 'higher';

export interface PlotPoint {
	t: number;
	value: number;
	x: number;
	y: number;
}

export interface Plot {
	points: PlotPoint[];
	min: number;
	max: number;
	flat: boolean;
	path: string;
	area: string;
}

const MINUS = '−';
const INSET = 2;

/** Signed whole-number delta with a real minus sign, e.g. `−61`, `+4`. */
export function formatSignedCount(delta: number): string {
	const rounded = Math.round(delta);
	return `${rounded < 0 ? MINUS : '+'}${Math.abs(rounded).toLocaleString('en-US')}`;
}

/** Signed percentage-point delta for a share held as 0..1, e.g. `−0.6 pts`. */
export function formatSignedPoints(delta: number): string {
	const points = Math.round(delta * 1000) / 10;
	return `${points < 0 ? MINUS : '+'}${Math.abs(points).toFixed(1)} pts`;
}

export function formatSignedDecimal(delta: number): string {
	const rounded = Math.round(delta * 100) / 100;
	return `${rounded < 0 ? MINUS : '+'}${Math.abs(rounded).toFixed(2)}`;
}

export function formatDay(ms: number): string {
	return new Date(ms).toLocaleDateString('en-GB', { day: 'numeric', month: 'short' });
}

export function formatRun(ms: number): string {
	const date = new Date(ms);
	const time = date.toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' });
	return `${formatDay(ms)} ${time}`;
}

export type ChangeKind = 'down' | 'up' | 'flat' | 'single';
export type ChangeTone = 'strong' | 'warning' | 'quiet';

export interface Change {
	kind: ChangeKind;
	tone: ChangeTone;
	label: string;
}

export function summariseChange(
	points: TrendPoint[],
	better: TrendDirection | undefined,
	formatDelta: (delta: number) => string
): Change {
	if (points.length < 2) return { kind: 'single', tone: 'quiet', label: 'one run so far' };
	const first = points[0]!;
	const last = points[points.length - 1]!;
	const since = formatDay(Date.parse(first.at));
	const delta = last.value - first.value;
	const text = formatDelta(delta);
	if (delta === 0 || text === formatDelta(0)) {
		return { kind: 'flat', tone: 'quiet', label: `no change since ${since}` };
	}
	const kind: ChangeKind = delta < 0 ? 'down' : 'up';
	if (!better) return { kind, tone: 'quiet', label: `${text} since ${since}` };
	const improved = better === 'lower' ? delta < 0 : delta > 0;
	return {
		kind,
		tone: improved ? 'strong' : 'warning',
		label: `${text} since ${since}, ${improved ? 'better' : 'worse'}`
	};
}

/** Step-after path and area for the points, in a viewBox 100 wide and `height` tall. */
export function buildPlot(points: TrendPoint[], domain: [number, number], height: number): Plot {
	const values = points.map((p) => p.value);
	const min = values.length > 0 ? Math.min(...values) : 0;
	const max = values.length > 0 ? Math.max(...values) : 0;
	const flat = min === max;
	const span = domain[1] - domain[0] || 1;
	const usable = height - INSET * 2;
	const plotted: PlotPoint[] = points.map((p) => {
		const t = Date.parse(p.at);
		return {
			t,
			value: p.value,
			x: Math.min(100, Math.max(0, ((t - domain[0]) / span) * 100)),
			y: flat ? height / 2 : INSET + usable - ((p.value - min) / (max - min)) * usable
		};
	});
	let path = '';
	let area = '';
	plotted.forEach((p, i) => {
		if (i === 0) path = `M${p.x} ${p.y}`;
		else path += ` H${p.x} V${p.y}`;
	});
	const first = plotted[0];
	const last = plotted[plotted.length - 1];
	if (first && last) area = `${path} V${height} H${first.x} Z`;
	return { points: plotted, min, max, flat, path, area };
}

/** The value in force at `at`: the newest point at or before it, else the first. */
export function valueAt(points: PlotPoint[], at: number): PlotPoint | null {
	let found: PlotPoint | null = null;
	for (const p of points) {
		if (p.t <= at) found = p;
		else break;
	}
	return found ?? points[0] ?? null;
}

export function nearestRun(times: number[], target: number): number | null {
	let best: number | null = null;
	for (const t of times) {
		if (best === null || Math.abs(t - target) < Math.abs(best - target)) best = t;
	}
	return best;
}
