import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { ALL_CATEGORIES, CATEGORY_META, severityOf, type Severity } from './diagnostics';
import type { DiagnosticCategory } from './types';

const EXPECTED_CATEGORIES = [
	'justifying_shortcuts',
	'deflecting_blame',
	'avoiding_work',
	'faking_understanding',
	'overselling'
] as const satisfies readonly DiagnosticCategory[];

describe('CATEGORY_META', () => {
	it('has metadata for every DiagnosticCategory variant', () => {
		for (const category of EXPECTED_CATEGORIES) {
			expect(CATEGORY_META[category]).toBeDefined();
			expect(CATEGORY_META[category].label).toBeTruthy();
			expect(CATEGORY_META[category].badgeClass).toBeTruthy();
		}
	});

	it('has no unexpected categories', () => {
		const keys = Object.keys(CATEGORY_META).sort();
		const expected = [...EXPECTED_CATEGORIES].sort();
		expect(keys).toEqual(expected);
	});

	it('ALL_CATEGORIES matches metadata keys', () => {
		expect(ALL_CATEGORIES.sort()).toEqual([...EXPECTED_CATEGORIES].sort());
	});

	it('every badgeClass references a class defined in app.css', () => {
		const cssPath = resolve(__dirname, '../app.css');
		const css = readFileSync(cssPath, 'utf-8');
		const declaredBadgeClasses = new Set<string>();
		const pattern = /\.badge-[a-z0-9-]+/g;
		for (const match of css.matchAll(pattern)) {
			declaredBadgeClasses.add(match[0].slice(1));
		}

		for (const anchor of ['badge-error', 'badge-warning', 'badge-muted']) {
			expect(declaredBadgeClasses.has(anchor)).toBe(true);
		}

		for (const category of EXPECTED_CATEGORIES) {
			const cls = CATEGORY_META[category].badgeClass;
			expect(
				declaredBadgeClasses.has(cls),
				`CATEGORY_META.${category}.badgeClass = "${cls}" is not declared in app.css`
			).toBe(true);
		}
	});
});

describe('severityOf', () => {
	const EXPECTED_SEVERITIES = ['error', 'warning', 'muted'] as const satisfies readonly Severity[];

	it('returns one of the three documented severity tiers for every category', () => {
		for (const category of EXPECTED_CATEGORIES) {
			const severity = severityOf(category);
			expect(EXPECTED_SEVERITIES).toContain(severity);
		}
	});

	it('maps each category to the documented tier (single source of truth)', () => {
		expect(severityOf('justifying_shortcuts')).toBe('warning');
		expect(severityOf('deflecting_blame')).toBe('warning');
		expect(severityOf('avoiding_work')).toBe('error');
		expect(severityOf('faking_understanding')).toBe('muted');
		expect(severityOf('overselling')).toBe('muted');
	});

	it('agrees with CATEGORY_META.badgeClass for every category', () => {
		const severityToBadge: Record<Severity, string> = {
			error: 'badge-error',
			warning: 'badge-warning',
			muted: 'badge-muted'
		};
		for (const category of EXPECTED_CATEGORIES) {
			const expected = severityToBadge[severityOf(category)];
			expect(CATEGORY_META[category].badgeClass).toBe(expected);
		}
	});
});
