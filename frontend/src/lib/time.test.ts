import { describe, it, expect } from 'vitest';
import {
	formatUptime,
	formatClockLabel,
	formatClockAbsolute,
	formatCompactCount,
	formatTimeSpent
} from './time';

describe('formatUptime', () => {
	it('renders days and zero-padded hours', () => {
		const seconds = 4 * 86_400 + 2 * 3_600 + 30 * 60;
		expect(formatUptime(seconds)).toBe('4d 02h');
	});

	it('renders 0d for an uptime under one day', () => {
		expect(formatUptime(3_600)).toBe('0d 01h');
	});

	it('clamps negative input to zero', () => {
		expect(formatUptime(-500)).toBe('0d 00h');
	});
});

describe('formatClockLabel', () => {
	const NOW = new Date(2026, 6, 10, 15, 30, 0).getTime();

	it('renders same-day timestamps as HH:MM', () => {
		expect(formatClockLabel('2026-07-10T09:05:00', NOW)).toBe('09:05');
	});

	it('renders yesterday as "Yst HH:MM"', () => {
		expect(formatClockLabel('2026-07-09T21:00:00', NOW)).toBe('Yst 21:00');
	});

	it('renders older timestamps as "Mon D · HH:MM"', () => {
		expect(formatClockLabel('2026-07-08T15:00:00', NOW)).toBe('Jul 8 · 15:00');
	});

	it('renders "—" for unparseable input', () => {
		expect(formatClockLabel(null, NOW)).toBe('—');
		expect(formatClockLabel('not-a-date', NOW)).toBe('—');
	});
});

describe('formatClockAbsolute', () => {
	it('renders "Mon D, HH:MM"', () => {
		expect(formatClockAbsolute('2026-07-08T15:00:00')).toBe('Jul 8, 15:00');
	});

	it('renders empty string for unparseable input', () => {
		expect(formatClockAbsolute(null)).toBe('');
		expect(formatClockAbsolute('nope')).toBe('');
	});
});

describe('formatCompactCount', () => {
	it('renders thousands-separated below 100,000, matching formatCount', () => {
		expect(formatCompactCount(0)).toBe('0');
		expect(formatCompactCount(123)).toBe('123');
		expect(formatCompactCount(67_137)).toBe('67,137');
	});

	it('renders K for 100,000 up to 1,000,000', () => {
		expect(formatCompactCount(123_456)).toBe('123.5K');
	});

	it('renders M for millions', () => {
		expect(formatCompactCount(16_119_915)).toBe('16.1M');
		expect(formatCompactCount(45_197_760)).toBe('45.2M');
	});

	it('renders B for billions', () => {
		expect(formatCompactCount(11_233_045_073)).toBe('11.2B');
	});

	it('renders "—" for null/undefined', () => {
		expect(formatCompactCount(null)).toBe('—');
		expect(formatCompactCount(undefined)).toBe('—');
	});
});

describe('formatTimeSpent', () => {
	it('keeps one decimal under ten seconds', () => {
		expect(formatTimeSpent(0.4)).toBe('0.4 s');
		expect(formatTimeSpent(9.3)).toBe('9.3 s');
	});

	it('uses whole seconds under a minute', () => {
		expect(formatTimeSpent(49)).toBe('49 s');
		expect(formatTimeSpent(9.97)).toBe('10 s');
	});

	it('uses minutes and padded seconds under an hour', () => {
		expect(formatTimeSpent(308)).toBe('5 min 08 s');
		expect(formatTimeSpent(544)).toBe('9 min 04 s');
	});

	it('rounds before choosing the unit', () => {
		expect(formatTimeSpent(59.6)).toBe('1 min 00 s');
		expect(formatTimeSpent(3_599.6)).toBe('1.0 h');
	});

	it('uses one decimal hours up to a hundred, whole hours after', () => {
		expect(formatTimeSpent(24.8 * 3_600)).toBe('24.8 h');
		expect(formatTimeSpent(108 * 3_600)).toBe('108 h');
		expect(formatTimeSpent(99.97 * 3_600)).toBe('100 h');
	});

	it('clamps negative and non-finite input to zero', () => {
		expect(formatTimeSpent(-5)).toBe('0.0 s');
		expect(formatTimeSpent(Number.NaN)).toBe('0.0 s');
	});
});
