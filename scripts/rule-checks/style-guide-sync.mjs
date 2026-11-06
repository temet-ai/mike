#!/usr/bin/env node
/**
 * style-guide-sync.mjs — assert the shipped palette matches the design system.
 *
 * `docs/frontend/STYLE_GUIDE.html` is the design export FUNK started from (the
 * style guide itself is Storybook's Foundations stories). Its
 * Forge block declares the dark palette. The app keeps one canonical copy:
 *
 *   1. the `:root` FUNK Layer-0 block in frontend/src/app.css  (--funk-*)
 *   2. the Forge `:root` block inside the style guide export itself
 *
 * The compatibility scope must alias surfaces and inherit text from the active
 * theme; redeclaring Forge literals there would break every other theme.
 * This compares the root palette against the guide and checks that inheritance
 * contract. It checks VALUES
 * ONLY — never contrast, in any form (see .claude/rules/visual-design-and-ui.md).
 *
 * It then checks that no extra copy has appeared. Instruction files that restate
 * a palette go stale and then get enforced against the real design: an agent
 * memory describing "six accent roles … teal" outlived the design system by
 * months and kept being recited as the whole palette. Prose cannot be synced, so
 * a colour value in a file that tells an agent how the app looks is a hard fail.
 * A deliberate never-do-this sample opts out with `palette-example` on the line.
 *
 * Usage:
 *   node scripts/rule-checks/style-guide-sync.mjs [--guide <path>]
 *   exit 0 = CLEAN, exit 1 = DRIFT, STRAY PALETTE, or unreadable input.
 */

import { readFileSync, existsSync, readdirSync, statSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve, relative, join } from 'node:path';

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const APP_CSS = resolve(REPO, 'frontend/src/app.css');

const guideFlag = process.argv.indexOf('--guide');
const GUIDE =
	guideFlag !== -1 && process.argv[guideFlag + 1]
		? resolve(process.cwd(), process.argv[guideFlag + 1])
		: resolve(REPO, 'docs/frontend/STYLE_GUIDE.html');

/* Guide token → its canonical palette token in app.css. */
const MAP = [
	['--bg', '--funk-bg'],
	['--surface-1', '--funk-s1'],
	['--surface-2', '--funk-s2'],
	['--surface-3', '--funk-s3'],
	['--line', '--funk-line'],
	['--line-strong', '--funk-line-strong'],
	['--text-1', '--funk-t1'],
	['--text-2', '--funk-t2'],
	['--text-3', '--funk-t3'],
	['--cyan', '--funk-cyan'],
	['--magenta', '--funk-magenta'],
	['--green', '--funk-green'],
	['--red', '--funk-red'],
	['--blue', '--funk-blue'],
	['--yellow', '--funk-yellow'],

	/* Session Flow identity — WHO a chip belongs to, added 2026-09-15. */
	['--vendor-anthropic', '--funk-vendor-anthropic'],
	['--ident-session-fg', '--funk-ident-session-fg'],
	['--ident-session-fill', '--funk-ident-session-fill'],
	['--ident-session-line', '--funk-ident-session-line'],
	['--ident-session-plate', '--funk-ident-session-plate'],
	['--ident-agent-fg', '--funk-ident-agent-fg'],
	['--ident-agent-fill', '--funk-ident-agent-fill'],
	['--ident-agent-line', '--funk-ident-agent-line'],
	['--ident-agent-plate', '--funk-ident-agent-plate'],
	['--ident-tool-fg', '--funk-ident-tool-fg'],
	['--ident-tool-fill', '--funk-ident-tool-fill'],
	['--ident-tool-line', '--funk-ident-tool-line'],
	['--ident-tool-plate', '--funk-ident-tool-plate'],
	['--ident-mike-fg', '--funk-ident-mike-fg'],
	['--ident-mike-fill', '--funk-ident-mike-fill'],
	['--ident-mike-line', '--funk-ident-mike-line'],
	['--ident-mike-plate', '--funk-ident-mike-plate'],
	['--ident-outside-fg', '--funk-ident-outside-fg'],
	['--ident-outside-fill', '--funk-ident-outside-fill'],
	['--ident-outside-line', '--funk-ident-outside-line'],
	['--ident-outside-plate', '--funk-ident-outside-plate'],
	['--ident-terminal-fg', '--funk-ident-terminal-fg'],
	['--ident-terminal-fill', '--funk-ident-terminal-fill'],
	['--ident-terminal-line', '--funk-ident-terminal-line'],
	['--ident-terminal-plate', '--funk-ident-terminal-plate']
];

const fail = (msg) => {
	console.error(`style-guide-sync: ${msg}`);
	process.exit(1);
};

for (const [label, path] of [
	['style guide', GUIDE],
	['app.css', APP_CSS]
]) {
	if (!existsSync(path)) fail(`${label} not found at ${path} — cannot verify the palette.`);
}

/** First `--token: <value>;` in `text`. The guide stores its CSS as an escaped
 *  string, so match on the token text rather than parsing a stylesheet. */
function readToken(text, token) {
	const m = text.match(new RegExp(`${token}\\s*:\\s*([^;\\\\]+)`));
	return m ? m[1].trim().toLowerCase() : null;
}

const guideRaw = readFileSync(GUIDE, 'utf8');
const forgeStart = guideRaw.indexOf('---- Forge (default) ----');
if (forgeStart === -1) fail('no "Forge (default)" block in the style guide — export shape changed.');
const daylight = guideRaw.indexOf('---- Daylight ----', forgeStart);
const forge = guideRaw.slice(forgeStart, daylight === -1 ? forgeStart + 4000 : daylight);

const appRaw = readFileSync(APP_CSS, 'utf8');
const dd = appRaw.indexOf("[data-design='funk']");
if (dd === -1) fail("no [data-design='funk'] block in app.css — did the scoped subtree move?");
const scoped = appRaw.slice(dd, appRaw.indexOf('\n}', dd));

const drift = [];
for (const [guideToken, funkToken] of MAP) {
	const want = readToken(forge, guideToken);
	if (!want) {
		drift.push([guideToken, 'MISSING from the guide', '', '']);
		continue;
	}
	const got = readToken(appRaw, funkToken);
	if (got !== want) drift.push([guideToken, want, `app.css ${funkToken}`, got ?? '(absent)']);
}

const aliases = [
	['--bg', '--surface-bg'],
	['--surface-1', '--surface-card'],
	['--line', '--border-default'],
	['--line-strong', '--border-strong']
];
for (const hue of ['cyan', 'magenta', 'green', 'red', 'blue', 'yellow']) {
	for (const suffix of ['', '-dim', '-border']) {
		aliases.push([`--${hue}${suffix}`, `--funk-${hue}${suffix}`]);
	}
	aliases.push([`--chip-${hue}`, `--funk-chip-${hue}`]);
}
for (const [token, source] of aliases) {
	const want = `var(${source})`;
	const got = readToken(scoped, token);
	if (got !== want) drift.push([token, want, 'FUNK compatibility scope', got ?? '(absent)']);
}
for (const token of ['--surface-2', '--surface-3', '--text-1', '--text-2', '--text-3']) {
	const got = readToken(scoped, token);
	if (got !== null) drift.push([token, 'inherited from theme', 'FUNK compatibility scope', got]);
}

/* Faces: a `--font-*` stack may only name families the app actually loads. The guide
 * embeds its own faces, so it can name any of them; app.css naming an unloaded family
 * silently drops that whole subtree to a system fallback (2026-07-30: the
 * `[data-design='funk']` block declared Orbitron/Michroma/Gugi and every memory panel
 * rendered in system fonts). Registration comes from the @fontsource imports in
 * +layout.svelte, which register `<Name> Variable`. */
const LAYOUT = resolve(REPO, 'frontend/src/routes/+layout.svelte');
/* A generic keyword is a last-resort fallback and never rescues a stack — `'Orbitron',
 * sans-serif` still loses the design. A PLATFORM name marks a deliberate
 * use-what-the-OS-has stack, which is allowed to bundle nothing. */
const GENERIC = new Set(['sans-serif', 'serif', 'monospace', 'cursive', 'fantasy']);
const PLATFORM = new Set([
	'system-ui',
	'ui-monospace',
	'ui-sans-serif',
	'ui-serif',
	'-apple-system',
	'blinkmacsystemfont',
	'segoe ui',
	'roboto',
	'helvetica neue',
	'arial'
]);

const loaded = new Set();
if (existsSync(LAYOUT)) {
	for (const m of readFileSync(LAYOUT, 'utf8').matchAll(
		/@fontsource(?:-variable)?\/([a-z0-9-]+)/g
	)) {
		const family = m[1].replace(/-/g, ' ');
		loaded.add(family);
		loaded.add(`${family} variable`);
	}
}

const unloaded = [];
if (loaded.size === 0) {
	unloaded.push(['(none)', 'no @fontsource imports found in +layout.svelte']);
} else {
	for (const m of appRaw.matchAll(/(--font-[a-z-]+)\s*:\s*([^;]+);/g)) {
		const [, token, stack] = m;
		if (stack.includes('var(')) continue;
		const families = stack
			.split(',')
			.map((p) => p.trim().replace(/^['"]|['"]$/g, '').toLowerCase())
			.filter(Boolean);
		if (families.some((f) => loaded.has(f) || PLATFORM.has(f))) continue;
		const named = families.find((f) => !GENERIC.has(f));
		if (named) unloaded.push([token, stack.replace(/\s+/g, ' ').trim()]);
	}
}

/* Stray palettes: a colour VALUE written into a file whose job is to tell an agent
 * how this app looks. Those files may name tokens; holding values makes them a
 * competing source of truth that nothing can keep in sync. */
const SCANNED = [
	'.claude/agents',
	'.claude/rules',
	'.claude/skills/ui-active-state-design',
	'.claude/skills/ui-coherence-state-audit',
	'.claude/skills/ui-assert',
	'CLAUDE.md',
	'docs/frontend/CONVENTIONS.md'
];
/* 6-digit hex, or a colour function with numeric args. Issue refs (#136) and
 * 3-digit forms are excluded deliberately — too many false positives to gate on. */
const COLOUR = /#[0-9A-Fa-f]{6}\b|\boklch\(\s*[\d.]|\brgba?\(\s*\d/;

function* walk(abs) {
	if (!existsSync(abs)) return;
	if (statSync(abs).isFile()) return yield abs;
	for (const entry of readdirSync(abs)) {
		if (entry === 'node_modules' || entry.startsWith('.git')) continue;
		yield* walk(join(abs, entry));
	}
}

/* The agent memory that caused this lives outside the repo and loads into every
 * session, so it outranks any in-repo file. Scan it when present on this machine;
 * its absence is not a failure. */
const MEMORY = resolve(
	process.env.HOME ?? '',
	'.claude/projects',
	REPO.replace(/\//g, '-'),
	'memory'
);

const stray = [];
for (const target of [...SCANNED, MEMORY]) {
	for (const file of walk(target.startsWith('/') ? target : resolve(REPO, target))) {
		if (!/\.(md|mdx)$/.test(file)) continue;
		if (/MEMORY\.md\.bak/.test(file)) continue;
		const rel = file.startsWith(REPO) ? relative(REPO, file) : file.replace(process.env.HOME, '~');
		readFileSync(file, 'utf8')
			.split('\n')
			.forEach((line, i) => {
				if (!COLOUR.test(line)) return;
				if (line.includes('palette-example')) return;
				if (/^\s*color:\s*\w+\s*$/.test(line)) return; // agent frontmatter chip colour
				stray.push([`${rel}:${i + 1}`, line.trim().slice(0, 88)]);
			});
	}
}

if (drift.length === 0 && unloaded.length === 0 && stray.length === 0) {
	console.log(
		`CLEAN — ${MAP.length} palette tokens match ${GUIDE.replace(REPO + '/', '')}; ` +
			'FUNK aliases preserve theme inheritance; ' +
			`every --font-* family is one of the ${loaded.size / 2} the app loads; ` +
			`no colour values outside the guide`
	);
	process.exit(0);
}

if (stray.length > 0) {
	console.error(
		`STRAY PALETTE — ${stray.length} colour value(s) in files that instruct agents:\n`
	);
	for (const [where, line] of stray) console.error(`  ${where}\n      ${line}`);
	console.error(
		`\nThese become a competing source of truth and go stale silently.` +
			`\nCite the token name and let ${relative(REPO, GUIDE)} hold the value,` +
			`\nor mark a deliberate never-do-this sample with "palette-example" on the line.\n`
	);
}

if (unloaded.length > 0) {
	console.error(`UNLOADED FACE — app.css names ${unloaded.length} family the app never loads:\n`);
	for (const [token, family] of unloaded) console.error(`  ${token.padEnd(18)} ${family}`);
	console.error(
		`\nAdd the @fontsource import to +layout.svelte, or use a family that is loaded.` +
			`\nAn unloaded family renders as a system fallback — silently, everywhere it applies.\n`
	);
}

if (drift.length === 0) process.exit(1);


console.error(`DRIFT — ${drift.length} palette value(s) disagree with the style guide:\n`);
for (const [token, want, where, got] of drift) {
	console.error(`  ${token.padEnd(14)} guide=${String(want).padEnd(10)} ${where} = ${got}`);
}
console.error(
	`\nThe guide is the source of truth for values. Update the app blocks to match,` +
		`\nor — if the operator approved a new value — change the guide and canonical app palette together.`
);
process.exit(1);
