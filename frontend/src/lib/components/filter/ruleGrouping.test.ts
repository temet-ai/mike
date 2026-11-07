import { describe, expect, it } from 'vitest';
import { buildFilterRule } from '$lib/test-support';
import type { CatalogueClass } from './promptCatalogue.svelte';
import {
	activeScopeRules,
	firesOnOpenRequest,
	groupFiringRules,
	groupOtherPromptRules,
	groupRulesByOtherProject,
	rulesMatchingNoPrompt
} from './ruleGrouping';

function buildClass(overrides: Partial<CatalogueClass> = {}): CatalogueClass {
	return {
		key: 'main:',
		lane: 'main',
		persona: null,
		title: 'Main session',
		registered: true,
		registrationLabel: 'registered',
		blockCount: 1,
		observationCount: 10,
		lastSeen: '2026-09-20T00:00:00Z',
		firstSeen: '2026-01-01T00:00:00Z',
		noRoute: false,
		models: [],
		versionsStatus: 'ready',
		versionsError: null,
		versions: [],
		...overrides
	};
}

describe('firesOnOpenRequest', () => {
	it('is true for an inject_reminder rule when enabled, regardless of firingRuleIds', () => {
		const rule = buildFilterRule({ id: 'r1', kind: 'inject_reminder' });
		expect(firesOnOpenRequest(rule, null)).toBe(true);
		expect(firesOnOpenRequest(rule, new Set(['other']))).toBe(true);
	});

	it('is false for a disabled inject_reminder rule', () => {
		const rule = buildFilterRule({ id: 'r1', kind: 'inject_reminder', enabled: false });
		expect(firesOnOpenRequest(rule, new Set(['r1']))).toBe(false);
	});

	it('is false for a disabled rule even when its id is in firingRuleIds', () => {
		const rule = buildFilterRule({ id: 'r1', enabled: false });
		expect(firesOnOpenRequest(rule, new Set(['r1']))).toBe(false);
	});

	it('reads firingRuleIds as the authoritative source for an enabled anchored rule', () => {
		const rule = buildFilterRule({ id: 'r1', enabled: true });
		expect(firesOnOpenRequest(rule, new Set(['r1']))).toBe(true);
		expect(firesOnOpenRequest(rule, new Set(['other']))).toBe(false);
		expect(firesOnOpenRequest(rule, null)).toBe(false);
	});
});

describe('groupFiringRules', () => {
	it('groups rules by their landing heading, preserving first-appearance order', () => {
		const block = '# Identity\nyou are claude, needle-a here\n\n# Tools\nneedle-b appears here';
		const ruleA = buildFilterRule({ id: 'a', kind: 'text_substring', value: 'needle-a' });
		const ruleB = buildFilterRule({ id: 'b', kind: 'text_substring', value: 'needle-b' });
		const ruleC = buildFilterRule({ id: 'c', kind: 'text_substring', value: 'needle-a' });

		const groups = groupFiringRules([ruleA, ruleB, ruleC], [block]);

		expect(groups.map((g) => g.heading)).toEqual(['Identity', 'Tools']);
		expect(groups[0]?.rules.map((r) => r.id)).toEqual(['a', 'c']);
		expect(groups[1]?.rules.map((r) => r.id)).toEqual(['b']);
	});

	it('never produces a "Not in this prompt" heading', () => {
		const rule = buildFilterRule({ id: 'a', kind: 'text_substring', value: 'absent-anchor' });
		const groups = groupFiringRules([rule], ['a block with no match at all']);
		expect(groups.map((g) => g.heading)).not.toContain('Not in this prompt');
	});
});

describe('groupOtherPromptRules', () => {
	it('keys a rule to every model · class whose latest sample contains its anchor', () => {
		const rule = buildFilterRule({ id: 'r1', kind: 'text_substring', value: 'anchor text' });
		const classes: CatalogueClass[] = [
			buildClass({
				key: 'main:',
				title: 'Main session',
				models: [
					{
						model: 'claude-opus-5',
						unscoped: false,
						windowedCount: 3,
						latestObservation: {
							requestId: '501',
							sessionId: 's1',
							createdAt: '2026-09-20T00:00:00Z'
						},
						sampleBlocks: [{ type: 'text', text: 'has anchor text right here' }],
						observationsTotalCount: 3,
						status: 'ready',
						error: null
					}
				]
			}),
			buildClass({
				key: 'subagent:Explore',
				lane: 'subagent',
				title: 'Explore subagent',
				models: [
					{
						model: 'claude-haiku-4-5',
						unscoped: false,
						windowedCount: 1,
						latestObservation: {
							requestId: '502',
							sessionId: 's2',
							createdAt: '2026-09-19T00:00:00Z'
						},
						sampleBlocks: [{ type: 'text', text: 'no relevant text here' }],
						observationsTotalCount: 1,
						status: 'ready',
						error: null
					}
				]
			})
		];

		const groups = groupOtherPromptRules([rule], classes);

		expect(groups).toHaveLength(1);
		expect(groups[0]).toMatchObject({
			key: 'claude-opus-5:main:',
			model: 'claude-opus-5',
			classTitle: 'Main session',
			latestRequestId: '501'
		});
		expect(groups[0]?.rules.map((r) => r.id)).toEqual(['r1']);
	});

	it('skips a model row whose sample is still loading (null sampleBlocks), never treating it as a non-match', () => {
		const rule = buildFilterRule({ id: 'r1', kind: 'text_substring', value: 'anchor text' });
		const classes: CatalogueClass[] = [
			buildClass({
				models: [
					{
						model: 'claude-opus-5',
						unscoped: false,
						windowedCount: 3,
						latestObservation: null,
						sampleBlocks: null,
						observationsTotalCount: null,
						status: 'loading',
						error: null
					}
				]
			})
		];
		expect(groupOtherPromptRules([rule], classes)).toEqual([]);
	});

	it('never matches an inject_reminder rule by content (it has no prompt anchor)', () => {
		const rule = buildFilterRule({ id: 'r1', kind: 'inject_reminder', value: 'anything' });
		const classes: CatalogueClass[] = [
			buildClass({
				models: [
					{
						model: 'claude-opus-5',
						unscoped: false,
						windowedCount: 1,
						latestObservation: null,
						sampleBlocks: [{ type: 'text', text: 'anything goes here' }],
						observationsTotalCount: 1,
						status: 'ready',
						error: null
					}
				]
			})
		];
		expect(groupOtherPromptRules([rule], classes)).toEqual([]);
	});
});

describe('rulesMatchingNoPrompt', () => {
	it('returns the rules not covered by any other-prompt group', () => {
		const covered = buildFilterRule({ id: 'covered' });
		const uncovered = buildFilterRule({ id: 'uncovered' });
		const groups = [
			{
				key: 'claude-opus-5:main:',
				model: 'claude-opus-5',
				classTitle: 'Main session',
				latestRequestId: '501',
				rules: [covered]
			}
		];
		expect(rulesMatchingNoPrompt([covered, uncovered], groups).map((r) => r.id)).toEqual([
			'uncovered'
		]);
	});
});

describe('activeScopeRules', () => {
	it('keeps global rules and the picked project’s, drops other projects’', () => {
		const global = buildFilterRule({ id: 'g1', project_path: null });
		const picked = buildFilterRule({ id: 'p1', project_path: '/mnt/data/projects/picked' });
		const other = buildFilterRule({ id: 'o1', project_path: '/mnt/data/projects/other' });
		const result = activeScopeRules([global, picked, other], '/mnt/data/projects/picked');
		expect(result.map((r) => r.id)).toEqual(['g1', 'p1']);
	});

	it('keeps only global rules when Global is picked', () => {
		const global = buildFilterRule({ id: 'g1', project_path: null });
		const scoped = buildFilterRule({ id: 's1', project_path: '/mnt/data/projects/other' });
		expect(activeScopeRules([global, scoped], null).map((r) => r.id)).toEqual(['g1']);
	});
});

describe('groupRulesByOtherProject', () => {
	it('groups rules scoped to a project other than the picked one, named from the projects list', () => {
		const a = buildFilterRule({ id: 'a1', project_path: '/mnt/data/projects/alpha' });
		const b1 = buildFilterRule({ id: 'b1', project_path: '/mnt/data/projects/beta' });
		const b2 = buildFilterRule({ id: 'b2', project_path: '/mnt/data/projects/beta' });
		const global = buildFilterRule({ id: 'g1', project_path: null });
		const picked = buildFilterRule({ id: 'p1', project_path: '/mnt/data/projects/picked' });

		const groups = groupRulesByOtherProject(
			[a, b1, b2, global, picked],
			'/mnt/data/projects/picked',
			[
				{ project_path: '/mnt/data/projects/alpha', custom_name: 'Alpha' },
				{ project_path: '/mnt/data/projects/beta', custom_name: null }
			]
		);

		expect(groups).toHaveLength(2);
		expect(groups[0]).toMatchObject({
			projectPath: '/mnt/data/projects/alpha',
			projectName: 'Alpha'
		});
		expect(groups[0]?.rules.map((r) => r.id)).toEqual(['a1']);
		expect(groups[1]).toMatchObject({
			projectPath: '/mnt/data/projects/beta',
			projectName: '/mnt/data/projects/beta'
		});
		expect(groups[1]?.rules.map((r) => r.id)).toEqual(['b1', 'b2']);
	});

	it('is empty when nothing is scoped to another project', () => {
		const global = buildFilterRule({ id: 'g1', project_path: null });
		const picked = buildFilterRule({ id: 'p1', project_path: '/mnt/data/projects/picked' });
		expect(groupRulesByOtherProject([global, picked], '/mnt/data/projects/picked', [])).toEqual([]);
	});
});
