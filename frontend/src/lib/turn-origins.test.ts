import { describe, it, expect } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
	TURN_ORIGINS,
	SIDECAR_ORIGINS,
	HOUSEKEEPING_ORIGINS,
	SIDECAR_BAND_LABELS,
	ORIGIN_CHIP_LABELS
} from './turn-origins';

const dirname = path.dirname(fileURLToPath(import.meta.url));
const MIGRATIONS_DIR = path.resolve(
	dirname,
	'../../../crates/mike/src/store/persistence/migrations'
);

function seededOriginCodes(): Set<string> {
	const files = readdirSync(MIGRATIONS_DIR).filter((f) => f.endsWith('.sql'));
	expect(files.length).toBeGreaterThan(0);

	const codes = new Set<string>();
	const insertRe = /INSERT\s+INTO\s+requests\.turn_origins[\s\S]*?;/gi;
	const tupleRe = /\(\s*\d+\s*,\s*'([a-z0-9_]+)'\s*\)/g;
	const deleteRe = /DELETE\s+FROM\s+requests\.turn_origins\s+WHERE\s+code\s*=\s*'([a-z0-9_]+)'/gi;

	for (const file of files) {
		const sql = readFileSync(path.join(MIGRATIONS_DIR, file), 'utf8');
		for (const stmt of sql.match(insertRe) ?? []) {
			for (const tuple of stmt.matchAll(tupleRe)) {
				const code = tuple[1];
				if (code !== undefined) codes.add(code);
			}
		}
		for (const del of sql.matchAll(deleteRe)) {
			const code = del[1];
			if (code !== undefined) codes.delete(code);
		}
	}
	return codes;
}

describe('turn-origin backend↔frontend parity', () => {
	it('frontend TURN_ORIGINS matches the codes seeded in backend migrations', () => {
		const backend = seededOriginCodes();
		const frontend = new Set<string>(TURN_ORIGINS);

		const missingFromFrontend = [...backend].filter((c) => !frontend.has(c)).sort();
		const missingFromBackend = [...frontend].filter((c) => !backend.has(c)).sort();

		expect(
			missingFromFrontend,
			'backend seeds these codes but the frontend union lacks them — add to TURN_ORIGINS (and decide sidecar/housekeeping/labels)'
		).toEqual([]);
		expect(
			missingFromBackend,
			'frontend lists these codes but no backend migration seeds them'
		).toEqual([]);
	});

	it('sidecar and housekeeping sets stay internally consistent', () => {
		const sidecars = new Set<string>(SIDECAR_ORIGINS);
		for (const h of HOUSEKEEPING_ORIGINS) {
			expect(sidecars.has(h)).toBe(true);
		}
		const excluded = SIDECAR_ORIGINS.filter((s) => !new Set<string>(HOUSEKEEPING_ORIGINS).has(s));
		expect(excluded).toEqual(['advisor_sidecar']);
		for (const code of ['primary', 'primary_first', 'subagent']) {
			expect(sidecars.has(code)).toBe(false);
		}
	});

	it('every sidecar has a non-empty band label; every non-primary origin has a chip', () => {
		for (const s of SIDECAR_ORIGINS) {
			expect(SIDECAR_BAND_LABELS[s].length).toBeGreaterThan(0);
		}
		for (const o of TURN_ORIGINS) {
			if (o === 'primary') {
				expect(ORIGIN_CHIP_LABELS[o]).toBe('');
			} else {
				expect(ORIGIN_CHIP_LABELS[o].length).toBeGreaterThan(0);
			}
		}
	});
});
