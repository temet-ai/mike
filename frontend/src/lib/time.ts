export function parseIso(value: string | null | undefined): Date | null {
	if (!value) return null;
	const ts = Date.parse(value);
	if (Number.isNaN(ts)) return null;
	return new Date(ts);
}

export function formatRelative(iso: string | null, now: number = Date.now()): string {
	const parsed = parseIso(iso);
	if (!parsed) return '—';
	const diffMs = now - parsed.getTime();
	if (diffMs < 1_000) return 'just now';
	const sec = Math.floor(diffMs / 1_000);
	if (sec < 60) return `${sec}s ago`;
	const min = Math.floor(sec / 60);
	if (min < 60) return `${min}m ago`;
	const hr = Math.floor(min / 60);
	if (hr < 24) return `${hr}h ago`;
	const day = Math.floor(hr / 24);
	return `${day}d ago`;
}

export function formatDuration(ms: number): string {
	const clamped = Math.max(0, ms);
	if (clamped < 10_000) {
		return `${(clamped / 1_000).toFixed(2)}s`;
	}
	if (clamped < 60_000) {
		return `${(clamped / 1_000).toFixed(1)}s`;
	}
	if (clamped < 3_600_000) {
		const min = Math.floor(clamped / 60_000);
		const sec = Math.floor((clamped % 60_000) / 1_000);
		return `${min}m ${sec.toString().padStart(2, '0')}s`;
	}
	const hr = Math.floor(clamped / 3_600_000);
	const min = Math.floor((clamped % 3_600_000) / 60_000);
	return `${hr}h ${min.toString().padStart(2, '0')}m`;
}

export function formatTimeSpent(seconds: number): string {
	const s = Number.isFinite(seconds) ? Math.max(0, seconds) : 0;
	const tenths = Math.round(s * 10) / 10;
	if (tenths < 10) return `${tenths.toFixed(1)} s`;
	const whole = Math.round(s);
	if (whole < 60) return `${whole} s`;
	if (whole < 3_600) {
		const min = Math.floor(whole / 60);
		return `${min} min ${String(whole % 60).padStart(2, '0')} s`;
	}
	const hours = Math.round((s / 3_600) * 10) / 10;
	return hours >= 100 ? `${Math.round(s / 3_600)} h` : `${hours.toFixed(1)} h`;
}

export function computeDurationMs(
	createdAt: string,
	completedAt: string | null,
	now: number = Date.now()
): number | null {
	const start = parseIso(createdAt);
	if (!start) return null;
	const end = parseIso(completedAt) ?? new Date(now);
	return end.getTime() - start.getTime();
}

export function formatCount(value: number | null | undefined): string {
	if (value === null || value === undefined) return '—';
	return value.toLocaleString('en-US');
}

export function formatCompactCount(value: number | null | undefined): string {
	if (value === null || value === undefined) return '—';
	const abs = Math.abs(value);
	if (abs < 100_000) return value.toLocaleString('en-US');
	if (abs < 1_000_000) return `${(value / 1_000).toFixed(1)}K`;
	if (abs < 1_000_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
	return `${(value / 1_000_000_000).toFixed(1)}B`;
}

export function formatUptime(seconds: number): string {
	const clamped = Math.max(0, Math.floor(seconds));
	const days = Math.floor(clamped / 86_400);
	const hours = Math.floor((clamped % 86_400) / 3_600);
	return `${days}d ${String(hours).padStart(2, '0')}h`;
}

export function formatClockLabel(iso: string | null | undefined, now: number): string {
	const target = parseIso(iso);
	if (!target) return '—';
	const clock = `${String(target.getHours()).padStart(2, '0')}:${String(target.getMinutes()).padStart(2, '0')}`;
	const nowDate = new Date(now);
	if (isSameLocalDay(target, nowDate)) return clock;
	const yesterday = new Date(now - 86_400_000);
	if (isSameLocalDay(target, yesterday)) return `Yst ${clock}`;
	const month = target.toLocaleDateString('en-US', { month: 'short' });
	return `${month} ${target.getDate()} · ${clock}`;
}

export function formatClockAbsolute(iso: string | null | undefined): string {
	const d = parseIso(iso);
	if (!d) return '';
	const month = d.toLocaleDateString('en-US', { month: 'short' });
	const clock = `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
	return `${month} ${d.getDate()}, ${clock}`;
}

function isSameLocalDay(a: Date, b: Date): boolean {
	return (
		a.getFullYear() === b.getFullYear() &&
		a.getMonth() === b.getMonth() &&
		a.getDate() === b.getDate()
	);
}
