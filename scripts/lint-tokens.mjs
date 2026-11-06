#!/usr/bin/env node
/**
 * lint-tokens.mjs — the Phase 0 token-contract gate (MULTI-THEME).
 *
 * Runs three checks against frontend/src/app.css and the component/route source,
 * with captured, exit-coded output (CLEAN or the hits — honest exit code):
 *
 *   1. MONOTONICITY — the neutral primitive ramp L values are strictly
 *      increasing (neutral-1 < 2 < 3 < 4 < 5 < 6 < 7 < 8 < 11 < 12 < 13 < 14),
 *      checked PER THEME (a theme that re-binds the neutral ramp must keep it
 *      monotonic too). For forge, the ramp lives in :root, so all themes that
 *      do not override a primitive inherit the same monotonic ramp. This is a
 *      ramp-ordering check, NOT a contrast check — it asserts the ramp steps
 *      stay in order, never that any pair clears a contrast floor.
 *
 *   2. NO PRIMITIVE IN COMPONENTS — `var(--primitive-*)` must not appear in
 *      frontend/src/lib/components or frontend/src/routes. Components read
 *      semantic roles only. (Phase 1 fully enforces this once migration lands;
 *      it already holds in Phase 0 because no component references primitives.)
 *
 *   3. NO color-mix() — `color-mix(` must not appear in component/route source.
 *      (Phase 1 removes the remaining app.css hover-tint sites; this gate keeps
 *      components clean from the start.)
 *
 * NOT CHECKED — colour contrast, in any form: no APCA Lc floors, no WCAG
 * ratios, no per-token pair assertions. Do not add them back; see
 * .claude/rules/visual-design-and-ui.md.
 *
 * THEME DISCOVERY: themes are discovered by scanning app.css for
 * `[data-theme="<name>"]` selectors. The forge theme is co-scoped with `:root`
 * (`:root, [data-theme='forge'] { ... }`), so it is always present. The other
 * ui-designer-authored themes (glacier, void, phosphor, crimson) are picked up
 * automatically the moment their `[data-theme]` blocks land in app.css — this
 * script does NOT hard-code a theme list, so it stays in sync with the CSS.
 *
 * PER-THEME RESOLUTION MODEL (mirrors the CSS cascade):
 *   - The BASE layer = every declaration in a block whose selector contains
 *     `:root` (the primitive ramps + the forge role defaults, which share the
 *     `:root, [data-theme='forge']` selector). Theme-agnostic primitives live
 *     here and are shared by every theme.
 *   - Each theme's effective map = BASE overlaid with that theme's OWN
 *     `[data-theme="x"]` block declarations. A theme that re-binds
 *     `--text-strong` to a different primitive (or a literal) shadows the base
 *     value; tokens it does not mention fall through to the base. This is
 *     exactly how `[data-theme]` (equal specificity, later source order) wins
 *     in the browser, so the gate resolves identically to what ships.
 *
 * Usage: node scripts/lint-tokens.mjs   (exit 0 = PASS, non-zero = FAIL)
 */

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve, join } from 'node:path';
import { importFrontend } from './lib/frontend-deps.mjs';

const culori = await importFrontend('culori');
const { converter } = culori.converter ? culori : culori.default;

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(__dirname, '..');
const APP_CSS = resolve(REPO, 'frontend/src/app.css');
const COMPONENT_DIRS = [
	resolve(REPO, 'frontend/src/lib/components'),
	resolve(REPO, 'frontend/src/routes')
];

const toRgb = converter('rgb');

const css = readFileSync(APP_CSS, 'utf8');

// ── CSS block parsing ─────────────────────────────────────────────────────────
// Split app.css into top-level `selector { body }` rule blocks so we can resolve
// each theme against its OWN declarations layered over the shared `:root` base.
// We only care about flat custom-property declarations inside these blocks;
// @theme inline / @custom-variant / @import / @source are skipped because they
// either don't carry role tokens or aren't part of the cascade we resolve.
//
// The parser is intentionally simple (brace-depth scan, not a full CSS AST):
// app.css's token blocks are flat `--name: value;` lists with no nested rules,
// and the regex-based declaration extraction inside each block tolerates the
// inline block comments the file uses heavily.

/** Extract top-level `{ ... }` rule blocks as { selector, body } pairs. */
function parseBlocks(source) {
	// Strip block comments first so a `{` or `}` inside a comment can't desync
	// the brace scanner. (Declaration values never contain bare braces here.)
	const stripped = source.replace(/\/\*[\s\S]*?\*\//g, '');
	const blocks = [];
	let i = 0;
	const n = stripped.length;
	while (i < n) {
		const open = stripped.indexOf('{', i);
		if (open === -1) break;
		const selector = stripped.slice(i, open).trim();
		// Find the matching close brace (handle nesting defensively, though the
		// token blocks are flat).
		let depth = 1;
		let j = open + 1;
		for (; j < n && depth > 0; j++) {
			if (stripped[j] === '{') depth++;
			else if (stripped[j] === '}') depth--;
		}
		const body = stripped.slice(open + 1, j - 1);
		blocks.push({ selector, body });
		i = j;
	}
	return blocks;
}

/** Pull `--name: value;` declarations from a rule body into a Map. */
function declsFromBody(body) {
	const map = new Map();
	const re = /(--[a-z0-9-]+)\s*:\s*([^;{}]+);/gi;
	let m;
	while ((m = re.exec(body)) !== null) {
		map.set(m[1].trim(), m[2].trim());
	}
	return map;
}

const BLOCKS = parseBlocks(css);

// A block contributes to the shared BASE layer if its selector references
// `:root` (the primitive ramps and the forge defaults both do). Everything
// else is a theme-specific override block.
function selectorIsBase(sel) {
	return /(^|[\s,])::?root\b/.test(sel) || /\broot\b/.test(sel.split(',')[0]);
}

// Discover theme names from every `[data-theme="x"]` / `[data-theme='x']`
// selector anywhere in the file. Order of first appearance is preserved so the
// report reads top-to-bottom like the CSS.
function discoverThemes(blocks) {
	const themes = [];
	const seen = new Set();
	const reSel = /\[data-theme\s*[~|^$*]?=\s*['"]([a-z0-9-]+)['"]\s*\]/gi;
	for (const { selector } of blocks) {
		let m;
		while ((m = reSel.exec(selector)) !== null) {
			const name = m[1];
			if (!seen.has(name)) {
				seen.add(name);
				themes.push(name);
			}
		}
	}
	return themes;
}

const THEMES = discoverThemes(BLOCKS);

// Build the shared BASE map (primitives + any role co-scoped with :root). The
// regex grabs declarations in source order; later declarations of the same name
// win, matching the cascade within the base layer.
function buildBaseMap(blocks) {
	const map = new Map();
	for (const { selector, body } of blocks) {
		if (!selectorIsBase(selector)) continue;
		for (const [k, v] of declsFromBody(body)) map.set(k, v);
	}
	return map;
}

// Build a theme's own override map: union of every block whose selector targets
// `[data-theme="<name>"]` (forge's overrides are already in the base via the
// shared `:root, [data-theme='forge']` selector, so this is empty for forge —
// which is correct, forge == base).
function buildThemeOverrides(blocks, theme) {
	const map = new Map();
	const reSel = new RegExp(`\\[data-theme\\s*[~|^$*]?=\\s*['"]${theme}['"]\\s*\\]`, 'i');
	for (const { selector, body } of blocks) {
		// Skip base blocks — those are the shared layer, not a theme override.
		// A `:root, [data-theme='forge']` block IS base; we don't want to count
		// it twice. Only count blocks that target this theme AND are not base.
		if (selectorIsBase(selector)) continue;
		if (!reSel.test(selector)) continue;
		for (const [k, v] of declsFromBody(body)) map.set(k, v);
	}
	return map;
}

const BASE_MAP = buildBaseMap(BLOCKS);

/**
 * Effective token map for a theme: BASE overlaid with the theme's overrides.
 * forge has no override block of its own (it shares :root), so forge === BASE.
 */
function themeMap(theme) {
	const map = new Map(BASE_MAP);
	for (const [k, v] of buildThemeOverrides(BLOCKS, theme)) map.set(k, v);
	return map;
}

// ── Check 1: monotonicity of the neutral ramp (per theme) ────────────────────
const NEUTRAL_STEPS = [1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 13, 14];

function checkMonotonicity(tokenMap, theme) {
	const errors = [];
	let prevL = -Infinity;
	let prevName = null;
	for (const step of NEUTRAL_STEPS) {
		const name = `--primitive-neutral-${step}`;
		const raw = tokenMap.get(name);
		if (!raw) {
			errors.push(`[${theme}] MISSING primitive ${name}`);
			continue;
		}
		const ok = toRgb(raw.trim());
		if (!ok) {
			errors.push(`[${theme}] UNPARSEABLE ${name}: ${raw}`);
			continue;
		}
		const oklch = converter('oklch')(raw.trim());
		const L = oklch.l;
		if (L <= prevL) {
			errors.push(
				`[${theme}] NON-MONOTONIC ${name} L=${L.toFixed(4)} not > ${prevName} L=${prevL.toFixed(4)}`
			);
		}
		prevL = L;
		prevName = name;
	}
	return errors;
}

// ── Checks 2 & 3: source greps ───────────────────────────────────────────────
function walk(dir) {
	const out = [];
	let entries;
	try {
		entries = readdirSync(dir);
	} catch {
		return out;
	}
	for (const e of entries) {
		const p = join(dir, e);
		const s = statSync(p);
		if (s.isDirectory()) out.push(...walk(p));
		else if (/\.(svelte|ts|css)$/.test(e)) out.push(p);
	}
	return out;
}

function grepSource(pattern, label) {
	const errors = [];
	const re = new RegExp(pattern);
	for (const dir of COMPONENT_DIRS) {
		for (const file of walk(dir)) {
			const lines = readFileSync(file, 'utf8').split('\n');
			lines.forEach((line, i) => {
				if (re.test(line)) {
					errors.push(`${label}  ${file.replace(REPO + '/', '')}:${i + 1}  ${line.trim()}`);
				}
			});
		}
	}
	return errors;
}

// ── Run ──────────────────────────────────────────────────────────────────────
// Check 1 (monotonicity) is the BLOCKING gate — the ramp-ordering contract,
// run for EVERY discovered theme. Checks 2 (no primitive in components) and 3
// (no color-mix) are the Phase-1 MIGRATION gate (spec §5 labels them "Phase 1
// gate"): in Phase 0 they are INFORMATIONAL — they report the migration backlog
// count but do not fail the build, because the 650-site rewire that removes
// those references is Phase 1's job. `--strict` promotes 2 & 3 to blocking;
// that is the flag Phase 1's per-batch reviewer flips on.
const STRICT = process.argv.includes('--strict');

const sections = [];
let failed = false;

function section(title, errors, { extra = null, blocking = true } = {}) {
	const ok = errors.length === 0;
	const enforced = blocking || STRICT;
	if (!ok && enforced) failed = true;
	const status = ok
		? 'PASS'
		: enforced
			? 'FAIL (' + errors.length + ')'
			: 'INFO — Phase 1 backlog (' + errors.length + ', not blocking in Phase 0)';
	let body = `\n## ${title}: ${status}\n`;
	if (extra) body += extra.map((l) => '   ' + l).join('\n') + '\n';
	// Always print the FULL hit list — a CI log is an inspection surface; never
	// elide the backlog. Informational (non-blocking) sections print in full too
	// so the operator sees the complete Phase-1 migration backlog at a glance.
	if (!ok) body += errors.map((e) => '   ' + e).join('\n') + '\n';
	sections.push(body);
}

// The forge theme is always present (co-scoped with :root). If discoverThemes
// somehow returns nothing (e.g. a future refactor that scopes forge purely to
// :root with no [data-theme] selector), fall back to forge so the gate still
// runs the base layer rather than silently checking nothing.
const themeList = THEMES.length > 0 ? THEMES : ['forge'];

for (const theme of themeList) {
	const map = themeMap(theme);

	const mono = checkMonotonicity(map, theme);
	section(`1. Monotonicity (neutral ramp) — ${theme}`, mono);
}

// Checks 2 & 3 are theme-agnostic (they grep component source, not tokens).
const primHits = grepSource('var\\(--primitive-', 'PRIMITIVE-IN-COMPONENT');
section('2. No var(--primitive-*) in components/routes', primHits, { blocking: false });

const mixHits = grepSource('color-mix\\(', 'COLOR-MIX');
section('3. No color-mix() in components/routes', mixHits, { blocking: false });

console.log(`lint-tokens.mjs — token-contract gate${STRICT ? ' (--strict: Phase 1 mode)' : ''}`);
console.log(
	`themes discovered in app.css: ${themeList.join(', ')}` +
		(THEMES.length === 0 ? '  (no [data-theme] selector found — defaulted to forge)' : '')
);
console.log(sections.join(''));
console.log(failed ? '\nRESULT: FAIL' : '\nRESULT: PASS');
process.exit(failed ? 1 : 0);
