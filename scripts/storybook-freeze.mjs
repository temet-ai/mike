#!/usr/bin/env node
/**
 * storybook-freeze.mjs — the DEV-SERVER freeze/dev-only-error gate.
 *
 * WHY THIS EXISTS
 *   `scripts/sb-test` (the vitest browser runner) and `storybook dev` (:6006)
 *   are two different environments. A story can pass sb-test yet hard-FREEZE
 *   the dev canvas (a synchronous reactive loop pegs the main thread — the
 *   console won't even open), or log dev-only errors sb-test never sees
 *   (state_unsafe_mutation on a real $state singleton mutated in a `{@const}`,
 *   `@vitest/browser/context can be imported only inside Browser Mode`, an
 *   effect_update_depth loop). sb-test cannot catch those — its runtime IS the
 *   browser-mode runner with the stub aliases. This gate loads each story in
 *   the actual `storybook dev` server with real Chromium and checks that the
 *   page still RESPONDS (a frozen page fails a tiny `evaluate` within 3s).
 *
 *   Born from the 2026-06-20 SessionTimeline freeze: a batch-warm $effect that
 *   read `bandSlices` and wrote `perBandTurn` looped forever on any non-ready
 *   band. It was invisible to sb-test and only showed as a frozen dev page.
 *
 * USAGE (Storybook dev must be running — the stack keeps it up as `mike-stack-storybook`; without systemd, `pnpm run storybook` in frontend/):
 *   node scripts/storybook-freeze.mjs                 # strided sample (~40 components) — fast smoke gate
 *   node scripts/storybook-freeze.mjs --all           # EVERY story (thorough, slow — serial)
 *   node scripts/storybook-freeze.mjs --sample 80     # N strided components
 *   node scripts/storybook-freeze.mjs sessiontimeline connectionbanner  # only ids containing these substrings
 *   STORYBOOK_URL=http://localhost:6007 node scripts/storybook-freeze.mjs
 *
 * EXIT CODE: non-zero if ANY story FROZE, rendered EMPTY, or logged a dev-only
 *   loop/context error — so it can gate CI. 2 if Storybook dev isn't reachable.
 *
 * Serial by design (fresh browser per story so one renderer crash can't cascade,
 * and to avoid the parallel-chromium thrash documented in
 * debugging-storybook-vitest-browser-thrash). Pair with sb-test, don't replace
 * it: sb-test is the play-function/a11y/typecheck gate; this is the freeze gate.
 */
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
// Resolve Playwright from frontend/node_modules (it's a frontend devDependency;
// repo-root has no node_modules). Playwright is CJS, so require() works.
const require = createRequire(path.join(repoRoot, 'frontend', 'package.json'));
const { chromium } = require('playwright');

const SB = process.env.STORYBOOK_URL || 'http://localhost:6006';
const LOOP_RE = /state_unsafe_mutation|effect_update_depth_exceeded|Maximum update depth|@vitest\/browser\/context/;

const rawArgs = process.argv.slice(2);
const all = rawArgs.includes('--all');
const sampleIdx = rawArgs.indexOf('--sample');
const sampleN = sampleIdx >= 0 ? Number(rawArgs[sampleIdx + 1]) : 40;
const substrings = rawArgs.filter((a) => !a.startsWith('--') && a !== String(sampleN));

let index;
try {
	index = await (await fetch(`${SB}/index.json`)).json();
} catch {
	console.error(`✗ Storybook dev not reachable at ${SB}/index.json`);
	console.error(`  Start it first:  systemctl --user start mike-stack-storybook  (without systemd: cd frontend && pnpm run storybook)`);
	console.error(`  Or set STORYBOOK_URL if it runs on another port.`);
	process.exit(2);
}

const entries = index.entries || {};
const storyIds = Object.keys(entries).filter((id) => entries[id].type === 'story');

let ids;
if (substrings.length) {
	ids = storyIds.filter((id) => substrings.some((s) => id.includes(s)));
} else if (all) {
	ids = storyIds;
} else {
	// One representative story per component, strided to ~sampleN across the tree.
	const seen = new Set();
	const comps = [];
	for (const id of storyIds) {
		const t = entries[id].title;
		if (!seen.has(t)) {
			seen.add(t);
			comps.push(id);
		}
	}
	const stride = Math.max(1, Math.floor(comps.length / sampleN));
	ids = comps.filter((_, i) => i % stride === 0).slice(0, sampleN);
}

if (ids.length === 0) {
	console.error('No matching stories.');
	process.exit(2);
}

const race = (p, ms, tag) => Promise.race([p, new Promise((r) => setTimeout(() => r(tag), ms))]);
const bad = [];
const stats = { OK: 0, EMPTY: 0, FROZEN: 0, 'LOOP-ERR': 0 };
console.log(`storybook-freeze: ${ids.length} stories @ ${SB}\n`);

const browser = await chromium.launch();
for (const id of ids) {
	let crashed = false;
	let loopErr = null;
	let root = -1;
	let responsive = null;
	const page = await browser.newPage();
	page.on('crash', () => (crashed = true));
	const noteErr = (text) => {
		if (!loopErr && LOOP_RE.test(text)) loopErr = text.replace(/\s+/g, ' ').slice(0, 120);
	};
	page.on('pageerror', (e) => noteErr(e.message));
	page.on('console', (m) => {
		if (/error|warn/i.test(m.type())) noteErr(m.text());
	});
	try {
		await page.goto(`${SB}/iframe.html?id=${id}&viewMode=story`, {
			waitUntil: 'domcontentloaded',
			timeout: 12000
		});
	} catch {
		/* nav timeout falls through to the responsiveness/root checks */
	}
	await new Promise((r) => setTimeout(r, 1500));
	responsive = await race(page.evaluate(() => 1).then(() => 'y').catch(() => 'e'), 3000, 'FROZEN');
	root = await race(page.$eval('#storybook-root', (el) => el.innerHTML.length).catch(() => -1), 2500, -2);
	await page.close().catch(() => {});

	const verdict = crashed || responsive === 'FROZEN' ? 'FROZEN' : loopErr ? 'LOOP-ERR' : root > 0 ? 'OK' : 'EMPTY';
	stats[verdict] = (stats[verdict] || 0) + 1;
	if (verdict !== 'OK') bad.push(`${verdict.padEnd(9)} ${id}${loopErr ? '  :: ' + loopErr : ''}`);
	console.log(`${verdict.padEnd(9)} ${id.slice(0, 56)}`);
}
await browser.close();

console.log(`\nSUMMARY ${JSON.stringify(stats)}`);
if (bad.length) {
	console.log('\nFAILURES:');
	for (const b of bad) console.log('  ' + b);
	process.exit(1);
}
console.log('✓ no freezes / dev-only errors');
