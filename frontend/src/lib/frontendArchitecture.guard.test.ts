import { describe, expect, it } from 'vitest';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { checkCatalogue } from '../../../scripts/rule-checks/component-families.mjs';
import { checkComposition } from '../../../scripts/rule-checks/page-composition.mjs';
import { checkMotionInLayer } from '../../../scripts/rule-checks/motion-in-layer.mjs';

function runCheck(script: string): { ok: boolean; output: string } {
	const file = fileURLToPath(new URL(`../../../scripts/${script}`, import.meta.url));
	try {
		return { ok: true, output: execFileSync('node', [file, '--check'], { encoding: 'utf8' }) };
	} catch (error) {
		const failure = error as { stdout?: string; stderr?: string };
		return { ok: false, output: `${failure.stdout ?? ''}${failure.stderr ?? ''}` };
	}
}

const sources = import.meta.glob('/src/**/*.{ts,svelte,css}', {
	query: '?raw',
	import: 'default',
	eager: true
}) as Record<string, string>;

const productionCodeSources = Object.entries(sources).filter(
	([path]) => path.endsWith('.ts') || path.endsWith('.svelte')
);
const styleSources = Object.entries(sources).filter(
	([path]) => path.endsWith('.css') || path.endsWith('.svelte')
);

const TEST_OR_STORY = /\.(test|spec)\.ts$|\.stories\.svelte$/;

const GROUPING_ALLOWED = ['/src/lib/grouping.ts'];
const SORT_BY_LAST_SEEN = /\.sort\(\s*\([^)]*\)\s*=>\s*[\s\S]{0,200}?last_seen/;

function isGroupingExempt(path: string): boolean {
	return GROUPING_ALLOWED.some((allowed) => path.endsWith(allowed)) || TEST_OR_STORY.test(path);
}

const ACTIVITY_CONSTANTS = ['RECENT_ACTIVE_WINDOW_MS', 'LIVE_STREAMING_STALE_MS'];
const ACTIVITY_ALLOWED = ['/src/lib/stores/sessions.svelte.ts', '/src/lib/sessionActivity.ts'];

function isActivityExempt(path: string): boolean {
	return ACTIVITY_ALLOWED.some((allowed) => path.endsWith(allowed)) || TEST_OR_STORY.test(path);
}

const INTERACTION = /:hover|:focus(-visible)?|:active(?![\w-])/;
const SELECTION =
	/\.selected\b|--active\b|--live\b|--current\b|\[aria-current|\[aria-pressed|:checked\b/;
const DATA_STATE =
	/--error\b|--loading\b|--disabled\b|--reverted\b|--success\b|--warning\b|--info\b|--danger\b|--dirty\b|--muted\b|--empty\b/;

const BANNED =
	/^\s*(padding(-(left|right|top|bottom))?|margin(-(left|right|top|bottom))?|font-weight|gap|border-width|border(-(left|right|top|bottom))?-width|border(-(left|right|top|bottom))?)\s*:/;

function styleText(path: string, src: string): string {
	if (path.endsWith('.css')) return src;
	return [...src.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)].map((m) => m[1] ?? '').join('\n');
}

function* rules(css: string): Generator<{ sel: string; body: string }> {
	const noComments = css.replace(/\/\*[\s\S]*?\*\//g, '');
	for (const m of noComments.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
		const sel = m[1];
		const body = m[2];
		if (sel === undefined || body === undefined) continue;
		yield { sel: sel.trim(), body };
	}
}

function offendersIn(path: string, src: string): string[] {
	const out: string[] = [];
	for (const { sel, body } of rules(styleText(path, src))) {
		const isState = INTERACTION.test(sel) || SELECTION.test(sel);
		if (!isState || DATA_STATE.test(sel)) continue;
		if (/::(before|after)/.test(sel) && /position\s*:\s*absolute/.test(body)) continue;
		for (const decl of body.split(';')) {
			if (!BANNED.test(decl + ';')) continue;
			const d = decl.trim();
			if (/^border(-(left|right|top|bottom))?\s*:/.test(d) && !/\d/.test(d)) continue;
			out.push(`${sel.replace(/\s+/g, ' ')}  ->  ${d}`);
		}
	}
	return out;
}

describe('list-ordering drift guard', () => {
	it('found production sources to scan (glob is wired up)', () => {
		expect(productionCodeSources.length).toBeGreaterThan(50);
	});

	it('nothing sorts on the mutable `last_seen` field outside $lib/grouping.ts', () => {
		const offenders = productionCodeSources
			.filter(([path]) => !isGroupingExempt(path))
			.filter(([, source]) => SORT_BY_LAST_SEEN.test(source))
			.map(([path]) => path);

		expect(
			offenders,
			'A .sort() comparator reads the mutable `last_seen` field outside ' +
				'$lib/grouping.ts. Group/order via groupByProject() or ' +
				'compareByFirstSeenDesc() instead. Offenders:\n' +
				offenders.join('\n')
		).toEqual([]);
	});
});

describe('session-activity drift guard', () => {
	it('found production sources to scan (glob is wired up)', () => {
		expect(productionCodeSources.length).toBeGreaterThan(50);
	});

	for (const constant of ACTIVITY_CONSTANTS) {
		it(`${constant} is referenced ONLY in the canonical activity module`, () => {
			const offenders = productionCodeSources
				.filter(([path]) => !isActivityExempt(path))
				.filter(([, source]) => source.includes(constant))
				.map(([path]) => path);

			expect(
				offenders,
				`${constant} leaked outside $lib/sessionActivity. Derive activity via ` +
					`deriveSessionActivity() instead of re-implementing window math. Offenders:\n` +
					offenders.join('\n')
			).toEqual([]);
		});
	}
});

describe('layout-stability guard (no shift on state change)', () => {
	it('scanned the stylesheets (glob is wired up)', () => {
		expect(styleSources.length).toBeGreaterThan(50);
	});

	it('no interaction/selection state rule changes a layout-affecting property', () => {
		const all: string[] = [];
		for (const [path, src] of styleSources) {
			if (path.endsWith('.stories.svelte')) continue;
			for (const hit of offendersIn(path, src)) all.push(`${path}\n   [${hit}]`);
		}
		expect(
			all,
			'A state change (hover/focus/active/selected) alters layout and will shift content. ' +
				'Convert to a paint-only accent (box-shadow / border-color / outline) — see ' +
				'.claude/rules/visual-design-and-ui.md. Offenders:\n' +
				all.join('\n')
		).toEqual([]);
	});
});

describe('component-family duplicate guard', () => {
	it('every component-name family with 2+ members is documented in the catalogue', () => {
		const { ok, problems } = checkCatalogue();
		expect(
			ok,
			'A component-name-suffix family (Card, Panel, Row, Strip, Section, Band, ' +
				'Badge/Pill/Chip/Tag, Header, Table, List, Banner, Drawer, Popover, Indicator, ' +
				'Marker) gained a member undocumented in docs/frontend/COMPONENT_CATALOGUE.md, or ' +
				"that catalogue names a file that no longer exists. Extend the family's canonical " +
				'component instead of adding a new file, or document the new file with a real reason. ' +
				'Problems:\n' +
				problems.join('\n')
		).toBe(true);
	});
});

describe('page composition guard', () => {
	it('no page restyles by hand more than its recorded count', () => {
		const { ok, problems } = checkComposition();
		expect(
			ok,
			'A page added a raw control, its own type styling, or its own augmented-ui chrome. ' +
				'Import the component instead: `scripts/component-index <word>` names it. ' +
				'Problems:\n' +
				problems.join('\n')
		).toBe(true);
	});
});

describe('motion stays in the shared layer', () => {
	it('no component writes more motion by hand than its recorded count', () => {
		const { ok, problems } = checkMotionInLayer();
		expect(
			ok,
			'A component added a transition, an animation, keyframes or a transition with a literal ' +
				'duration. Motion lives in src/lib/components/ui/motion.css and motion.ts; a component ' +
				'names the motion it takes with data-motion. Problems:\n' +
				problems.join('\n')
		).toBe(true);
	});
});

describe('style guide files an agent reads', () => {
	it('the rendered guide was made from the current export', () => {
		const { ok, output } = runCheck('style-guide-render');
		expect(ok, output).toBe(true);
	});

	it('the pattern-to-component index matches the components on disk', () => {
		const { ok, output } = runCheck('component-index');
		expect(ok, output).toBe(true);
	});
});
