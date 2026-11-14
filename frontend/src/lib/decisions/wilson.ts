const Z = 1.96;

export interface Interval {
	low: number;
	high: number;
}

export function wilson(successes: number, total: number): Interval | null {
	if (!(total > 0)) return null;
	const p = successes / total;
	const z2 = Z * Z;
	const centre = (p + z2 / (2 * total)) / (1 + z2 / total);
	const half = (Z * Math.sqrt((p * (1 - p)) / total + z2 / (4 * total * total))) / (1 + z2 / total);
	return { low: Math.max(0, centre - half), high: Math.min(1, centre + half) };
}

export function formatInterval(interval: Interval | null): string {
	if (interval === null) return '—';
	return `${interval.low.toFixed(2)}–${interval.high.toFixed(2)}`;
}
