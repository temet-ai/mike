#!/usr/bin/env node
/**
 * storybook-test-all-themes.mjs — per-theme Storybook suite matrix.
 *
 * Runs the `storybook` vitest project once per theme discovered in
 * frontend/src/app.css, forcing each pass to a single theme via the
 * `STORYBOOK_THEME` env var (which preview.ts reads to set the active
 * `data-theme` synchronously — see the spike note in vitest.config.ts).
 *
 * This is research Strategy 1 ("two vitest passes with initialGlobals", here
 * generalised to N themes): the most robust per-theme gate given the
 * storybook#31228 per-story-globals-propagation caveat. Each pass is a clean
 * full-suite run under one theme's surface, so a story that renders correctly
 * in forge but breaks in (say) glacier still fails the build.
 *
 * Theme discovery is identical to scripts/lint-tokens.mjs: parse every
 * `[data-theme="x"]` selector in app.css. Adding a theme block to app.css adds
 * a matrix pass automatically — no edit here.
 *
 * Usage:
 *   node scripts/storybook-test-all-themes.mjs            # all discovered themes
 *   node scripts/storybook-test-all-themes.mjs forge void # only the named themes
 *
 * Exit code: non-zero if ANY theme's pass fails (first failure stops the run).
 *
 * NOTE: this is the browser gate. scripts/lint-tokens.mjs covers the token
 * contract without a browser; this matrix adds real-DOM rendering plus axe's
 * structural rules under each theme on top.
 */

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(__dirname, '..');
const FRONTEND = resolve(REPO, 'frontend');
const APP_CSS = resolve(FRONTEND, 'src/app.css');

/**
 * Discover theme names from `[data-theme="x"]` selectors in app.css.
 *
 * Strip block comments FIRST so doc/example references inside comments
 * (e.g. `[data-theme="midnight"]` in an explanatory note) are NOT mistaken for
 * real theme blocks. Matches the discovery in scripts/lint-tokens.mjs, which
 * parses block selectors after comment-stripping for the same reason.
 */
function discoverThemes(source) {
	const stripped = source.replace(/\/\*[\s\S]*?\*\//g, '');
	const themes = [];
	const seen = new Set();
	const re = /\[data-theme\s*[~|^$*]?=\s*['"]([a-z0-9-]+)['"]\s*\]/gi;
	let m;
	while ((m = re.exec(stripped)) !== null) {
		if (!seen.has(m[1])) {
			seen.add(m[1]);
			themes.push(m[1]);
		}
	}
	return themes;
}

const css = readFileSync(APP_CSS, 'utf8');
const discovered = discoverThemes(css);
const requested = process.argv.slice(2);

let themes = requested.length > 0 ? requested : discovered;
if (themes.length === 0) themes = ['forge']; // forge is co-scoped with :root

// Validate requested themes exist in app.css (catch typos early).
if (requested.length > 0) {
	const missing = requested.filter((t) => !discovered.includes(t));
	if (missing.length > 0) {
		console.error(
			`storybook-test-all-themes: requested theme(s) not found in app.css: ${missing.join(', ')}\n` +
				`  discovered: ${discovered.join(', ') || '(none)'}`
		);
		process.exit(2);
	}
}

console.log(`storybook-test-all-themes — per-theme suite matrix`);
console.log(`themes: ${themes.join(', ')}\n`);

const failures = [];
for (const theme of themes) {
	console.log(`\n──────── THEME: ${theme} ────────`);
	const res = spawnSync('pnpm', ['exec', 'vitest', 'run', '--project', 'storybook'], {
		cwd: FRONTEND,
		stdio: 'inherit',
		env: { ...process.env, STORYBOOK_THEME: theme }
	});
	if (res.status !== 0) {
		failures.push(theme);
		console.error(`\n✗ THEME ${theme} FAILED (exit ${res.status})`);
		// Stop at the first failing theme — the operator fixes it then re-runs.
		break;
	}
	console.log(`\n✓ THEME ${theme} passed`);
}

console.log('\n════════ SUMMARY ════════');
if (failures.length > 0) {
	console.log(`RESULT: FAIL — theme(s): ${failures.join(', ')}`);
	process.exit(1);
}
console.log(`RESULT: PASS — all themes: ${themes.join(', ')}`);
process.exit(0);
