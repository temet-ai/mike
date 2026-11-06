import { describe, expect, it } from 'vitest';
import { readdirSync, statSync, existsSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROUTES_DIR = dirname(fileURLToPath(import.meta.url));

const KNOWN_GAPS: Record<string, string> = {};

function routePageDirs(dir: string): string[] {
	const out: string[] = [];
	for (const entry of readdirSync(dir)) {
		const p = join(dir, entry);
		if (statSync(p).isDirectory()) {
			out.push(...routePageDirs(p));
		} else if (entry === '+page.svelte') {
			out.push(dir);
		}
	}
	return out;
}

function hasStory(dir: string): boolean {
	if (existsSync(join(dir, 'page.stories.svelte'))) return true;
	return readdirSync(dir).some((f) => f.endsWith('.stories.svelte'));
}

describe('route → story parity', () => {
	const pageDirs = routePageDirs(ROUTES_DIR);

	it('discovers route page components', () => {
		expect(pageDirs.length).toBeGreaterThan(10);
	});

	it('every route has a Storybook story (or is a declared KNOWN_GAP)', () => {
		const missing: string[] = [];
		for (const dir of pageDirs) {
			const routePath = relative(ROUTES_DIR, dir).replace(/\\/g, '/');
			if (hasStory(dir)) continue;
			if (routePath in KNOWN_GAPS) continue;
			missing.push(routePath || '(index)');
		}
		expect(
			missing,
			`Routes missing a *.stories.svelte and not in KNOWN_GAPS:\n  ${missing.join(
				'\n  '
			)}\nAuthor a page.stories.svelte for each, or (with operator sign-off) add it to KNOWN_GAPS.`
		).toEqual([]);
	});

	it('KNOWN_GAPS only lists routes that genuinely lack a story (no stale entries)', () => {
		const stale: string[] = [];
		for (const routePath of Object.keys(KNOWN_GAPS)) {
			const dir = join(ROUTES_DIR, routePath);
			if (existsSync(join(dir, '+page.svelte')) && hasStory(dir)) {
				stale.push(routePath);
			}
		}
		expect(
			stale,
			`KNOWN_GAPS entries that now HAVE a story (remove them — the list may only shrink):\n  ${stale.join(
				'\n  '
			)}`
		).toEqual([]);
	});
});
