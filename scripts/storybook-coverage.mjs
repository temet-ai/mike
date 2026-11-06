#!/usr/bin/env node
/**
 * storybook-coverage.mjs
 *
 * Measures story coverage PER COMPONENT: every production Svelte component must
 * have its OWN sibling story — a file with the same basename stem plus
 * `.stories.svelte` in the same directory (Foo.svelte ↔ Foo.stories.svelte).
 *
 * A different component's story in the same folder does NOT count. This is the
 * operator-mandated definition (2026-06-17): "each and every component is
 * required to be covered". The previous per-directory logic was a bug that let
 * components ship storyless as long as a sibling in the folder had a story.
 *
 * Usage:
 *   node scripts/storybook-coverage.mjs            # human report, exit 0
 *   node scripts/storybook-coverage.mjs --json      # machine-readable, exit 0
 *   node scripts/storybook-coverage.mjs --gate      # exit 1 if any uncovered (CI gate)
 *
 * Definition:
 *   - component = *.svelte that is NOT *.stories.svelte, NOT a route file
 *     (+page/+layout/+error), NOT a test harness (basename ending in
 *     `Harness.svelte`, or anything under __harness__/), and NOT under
 *     __storybook-stubs__/. Harnesses are story/test scaffolding — they mount
 *     real components under controlled state and are not themselves a unit of
 *     UI requiring its own story.
 *   - covered   = a sibling <stem>.stories.svelte exists in the same directory.
 *   - route     = +page.svelte; covered when any *.stories.svelte exists in its
 *     own directory (SvelteKit page-story convention).
 */

import { readdirSync, statSync } from 'node:fs';
import { join, basename, dirname, extname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, '..', 'frontend', 'src');

const args = process.argv.slice(2);
const JSON_OUTPUT = args.includes('--json');
const GATE = args.includes('--gate');

/** Directories that never hold "components" for coverage purposes. */
const EXCLUDED_DIR_SEGMENTS = ['__harness__', '__storybook-stubs__'];

/**
 * Walk a directory tree and collect files matching predicate.
 * @param {string} dir
 * @param {(f: string) => boolean} predicate
 * @returns {string[]}
 */
function walk(dir, predicate) {
	const results = [];
	for (const entry of readdirSync(dir)) {
		const full = join(dir, entry);
		const stat = statSync(full);
		if (stat.isDirectory()) {
			results.push(...walk(full, predicate));
		} else if (predicate(full)) {
			results.push(full);
		}
	}
	return results;
}

const isExcludedDir = (f) => EXCLUDED_DIR_SEGMENTS.some((seg) => f.includes(`/${seg}/`));

// ── Collect all .svelte files ─────────────────────────────────────────────────
const allSvelteFiles = walk(ROOT, (f) => extname(f) === '.svelte');

const storyFiles = allSvelteFiles.filter((f) => f.endsWith('.stories.svelte'));
const routeFiles = allSvelteFiles.filter((f) => basename(f) === '+page.svelte');
const componentFiles = allSvelteFiles.filter(
	(f) =>
		!f.endsWith('.stories.svelte') &&
		!basename(f).startsWith('+') &&
		!basename(f).endsWith('Harness.svelte') &&
		!isExcludedDir(f)
);

// ── Per-component coverage: each component needs its OWN same-stem story ──────
// Key = "<dir>/<stem>" derived from every story file (stem = name minus .stories.svelte).
const ownStoryKeys = new Set(
	storyFiles.map((f) => join(dirname(f), basename(f).replace(/\.stories\.svelte$/, '')))
);
const componentKey = (f) => join(dirname(f), basename(f, '.svelte'));

const coveredComponents = componentFiles.filter((f) => ownStoryKeys.has(componentKey(f)));
const uncoveredComponents = componentFiles.filter((f) => !ownStoryKeys.has(componentKey(f)));

// ── Route page coverage (story in the route's own directory) ─────────────────
const storyDirs = new Set(storyFiles.map((f) => dirname(f)));
const coveredRoutes = routeFiles.filter((f) => storyDirs.has(dirname(f)));
const uncoveredRoutes = routeFiles.filter((f) => !storyDirs.has(dirname(f)));

// ── Stats ─────────────────────────────────────────────────────────────────────
const pct = (n, d) => (d > 0 ? Math.round((n / d) * 100) : 0);
const rel = (f) => f.replace(ROOT, '').replace(/^\//, '');

const stats = {
	mode: 'per-component',
	components: {
		total: componentFiles.length,
		covered: coveredComponents.length,
		uncovered: uncoveredComponents.length,
		coveragePercent: pct(coveredComponents.length, componentFiles.length)
	},
	routes: {
		total: routeFiles.length,
		covered: coveredRoutes.length,
		uncovered: uncoveredRoutes.length,
		coveragePercent: pct(coveredRoutes.length, routeFiles.length)
	},
	storyFiles: storyFiles.length,
	uncoveredList: uncoveredComponents.map(rel).sort(),
	uncoveredRouteList: uncoveredRoutes.map(rel).sort()
};

if (JSON_OUTPUT) {
	console.log(JSON.stringify(stats, null, 2));
} else {
	const hr = '─'.repeat(60);
	console.log('\nStorybook Coverage Report (per-component)');
	console.log(hr);
	console.log(
		`Components:  ${stats.components.covered} / ${stats.components.total}` +
			`  (${stats.components.coveragePercent}% covered)`
	);
	console.log(
		`Routes:      ${stats.routes.covered} / ${stats.routes.total}` +
			`  (${stats.routes.coveragePercent}% covered)`
	);
	console.log(`Story files: ${stats.storyFiles}`);
	console.log(hr);
	if (stats.uncoveredList.length > 0) {
		console.log(`\nUncovered components (${stats.components.uncovered}):`);
		for (const f of stats.uncoveredList) console.log(`  ${f}`);
	}
	if (stats.uncoveredRouteList.length > 0) {
		console.log(`\nUncovered routes (${stats.routes.uncovered}):`);
		for (const f of stats.uncoveredRouteList) console.log(`  ${f}`);
	}
	if (stats.uncoveredList.length === 0 && stats.uncoveredRouteList.length === 0) {
		console.log('\nAll components and routes have their own story. 100%.');
	}
	console.log('');
}

if (GATE && (uncoveredComponents.length > 0 || uncoveredRoutes.length > 0)) {
	console.error(
		`\nstorybook-coverage --gate FAILED: ` +
			`${uncoveredComponents.length} component(s) + ${uncoveredRoutes.length} route(s) ` +
			`lack their own story.`
	);
	process.exit(1);
}
