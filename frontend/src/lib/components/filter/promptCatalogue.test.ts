import { describe, expect, it } from 'vitest';
import { buildFilterRule, buildPromptClassDto } from '$lib/test-support';
import type {
	FilterRuleDto,
	ObservedSystemBlock,
	PromptClassDto,
	PromptClassVersionDto
} from '$lib/types';
import {
	applicabilityLabel,
	buildModelFirstTree,
	classForTurnOrigin,
	classKey,
	classTitle,
	extractModelReleases,
	isMainLanePersonaRow,
	isNeverObserved,
	parseReleaseFromBlockText,
	ruleApplicability,
	sortClasses,
	splitReleasesForDisplay,
	type ApplicabilityClass,
	type ModelFirstSourceClass
} from './promptCatalogue';

function block(text: string): ObservedSystemBlock {
	return { type: 'text', text };
}

describe('classKey', () => {
	it('keys the real main session by lane alone (null persona)', () => {
		expect(classKey('main', null)).toBe('main:');
	});

	it('keys a persona row by lane + persona, distinct from the null-persona row', () => {
		expect(classKey('main', 'steer-author')).toBe('main:steer-author');
		expect(classKey('main', 'steer-author')).not.toBe(classKey('main', null));
	});
});

describe('classTitle', () => {
	it('titles the real main session (main lane, null persona) "Main session"', () => {
		expect(classTitle('main', null)).toBe('Main session');
	});

	it('titles a main-lane persona row by the persona, never "Main session" — the fix for the "Main session appears twice" defect', () => {
		expect(classTitle('main', 'steer-author')).toBe('steer-author');
	});

	it('titles a subagent persona row by the persona', () => {
		expect(classTitle('subagent', 'Frontend Development Expert')).toBe(
			'Frontend Development Expert'
		);
	});

	it('titles the subagent unresolved-persona bucket "Unresolved persona"', () => {
		expect(classTitle('subagent', null)).toBe('Unresolved persona');
	});
});

describe('isMainLanePersonaRow', () => {
	it('is true for a main-lane row with a non-empty persona', () => {
		expect(isMainLanePersonaRow({ lane: 'main', subagent_persona: 'steer-author' })).toBe(true);
	});

	it('is false for the real main session (main lane, null persona)', () => {
		expect(isMainLanePersonaRow({ lane: 'main', subagent_persona: null })).toBe(false);
	});

	it('is false for a subagent-lane row, persona present or not', () => {
		expect(isMainLanePersonaRow({ lane: 'subagent', subagent_persona: 'X' })).toBe(false);
		expect(isMainLanePersonaRow({ lane: 'subagent', subagent_persona: null })).toBe(false);
	});
});

describe('isNeverObserved', () => {
	it('is true only when first_seen is null', () => {
		expect(isNeverObserved({ first_seen: null })).toBe(true);
		expect(isNeverObserved({ first_seen: '2026-07-01T00:00:00Z' })).toBe(false);
	});
});

describe('sortClasses', () => {
	const main = buildPromptClassDto({
		lane: 'main',
		subagent_persona: null,
		first_seen: '2026-08-01T00:00:00Z',
		last_seen: '2026-09-07T00:00:00Z'
	});
	const steerAuthor = buildPromptClassDto({
		lane: 'main',
		subagent_persona: 'steer-author',
		first_seen: null,
		last_seen: null
	});
	const rustDev = buildPromptClassDto({
		lane: 'subagent',
		subagent_persona: 'Rust Development Expert',
		first_seen: '2026-08-01T00:00:00Z',
		last_seen: '2026-09-01T00:00:00Z'
	});
	const frontendDev = buildPromptClassDto({
		lane: 'subagent',
		subagent_persona: 'Frontend Development Expert',
		first_seen: '2026-08-01T00:00:00Z',
		last_seen: '2026-09-01T00:00:00Z'
	});
	const uiDesigner = buildPromptClassDto({
		lane: 'subagent',
		subagent_persona: 'ui-designer',
		first_seen: '2026-08-01T00:00:00Z',
		last_seen: '2026-09-01T00:00:00Z'
	});
	const dbDev = buildPromptClassDto({
		lane: 'subagent',
		subagent_persona: 'db-dev',
		first_seen: '2026-08-01T00:00:00Z',
		last_seen: '2026-09-01T00:00:00Z'
	});
	const unresolvedSubagent = buildPromptClassDto({
		lane: 'subagent',
		subagent_persona: null,
		first_seen: '2026-08-01T00:00:00Z',
		last_seen: '2026-09-01T00:00:00Z'
	});
	const neverObservedSubagent = buildPromptClassDto({
		lane: 'subagent',
		subagent_persona: 'unit-tester',
		first_seen: null,
		last_seen: null
	});

	function titleKey(dto: PromptClassDto): string {
		return `${dto.lane}:${dto.subagent_persona ?? '(null)'}`;
	}

	it('orders main first, subagent personas alphabetically, unresolved persona last among observed, never-observed classes after observed ones within each lane', () => {
		const input = [
			neverObservedSubagent,
			unresolvedSubagent,
			dbDev,
			uiDesigner,
			frontendDev,
			rustDev,
			steerAuthor,
			main
		];
		const sorted = sortClasses(input).map(titleKey);
		expect(sorted).toEqual([
			'main:(null)',
			'main:steer-author',
			'subagent:db-dev',
			'subagent:Frontend Development Expert',
			'subagent:Rust Development Expert',
			'subagent:ui-designer',
			'subagent:(null)',
			'subagent:unit-tester'
		]);
	});

	it('does not mutate its input array', () => {
		const input = [rustDev, main];
		const copy = [...input];
		sortClasses(input);
		expect(input).toEqual(copy);
	});
});

describe('classForTurnOrigin', () => {
	it('maps primary_first to the main lane with no persona', () => {
		expect(classForTurnOrigin('primary_first', null)).toEqual({ lane: 'main', persona: null });
	});

	it('maps subagent to the subagent lane with the resolved persona', () => {
		expect(classForTurnOrigin('subagent', 'Frontend Development Expert')).toEqual({
			lane: 'subagent',
			persona: 'Frontend Development Expert'
		});
	});

	it('maps subagent with a missing persona to the subagent lane, null persona', () => {
		expect(classForTurnOrigin('subagent', undefined)).toEqual({ lane: 'subagent', persona: null });
	});

	it('maps workflow_subagent to the subagent lane, unresolved persona', () => {
		expect(classForTurnOrigin('workflow_subagent', 'ignored')).toEqual({
			lane: 'subagent',
			persona: null
		});
	});

	it('treats plain primary (a continuation, not a session first capture) as not catalogued', () => {
		expect(classForTurnOrigin('primary', null)).toBeNull();
	});

	it('treats sidecar origins and a missing turn_origin as not catalogued', () => {
		expect(classForTurnOrigin('title_sidecar', null)).toBeNull();
		expect(classForTurnOrigin(undefined, null)).toBeNull();
		expect(classForTurnOrigin(null, null)).toBeNull();
	});
});

describe('ruleApplicability', () => {
	const rule: FilterRuleDto = buildFilterRule({
		id: 'rule-1',
		kind: 'text_substring',
		value: 'banned phrase'
	});

	const classes: ApplicabilityClass[] = [
		{
			key: 'main:',
			title: 'Main session',
			models: [
				{ sampleBlocks: [block('this has a banned phrase in it')] },
				{ sampleBlocks: [block('clean text')] }
			]
		},
		{
			key: 'subagent:',
			title: 'Unresolved persona',
			models: [{ sampleBlocks: [block('also has a banned phrase')] }]
		},
		{
			key: 'subagent:ui-designer',
			title: 'ui-designer',
			models: [{ sampleBlocks: null }]
		}
	];

	it('counts matched vs. total sampled models per class, excluding rows with no sampled content', () => {
		const result = ruleApplicability(rule, classes);
		expect(result).toEqual([
			{ classKey: 'main:', title: 'Main session', matchedModels: 1, totalModels: 2 },
			{ classKey: 'subagent:', title: 'Unresolved persona', matchedModels: 1, totalModels: 1 }
		]);
	});

	it('omits a class entirely when it has zero sampled models', () => {
		const onlyUnsampled: ApplicabilityClass[] = [
			{ key: 'subagent:ui-designer', title: 'ui-designer', models: [{ sampleBlocks: null }] }
		];
		expect(ruleApplicability(rule, onlyUnsampled)).toEqual([]);
	});
});

describe('applicabilityLabel', () => {
	it('joins classes the rule matched, in order, with a "N of M models" count each', () => {
		expect(
			applicabilityLabel([
				{ classKey: 'main:', title: 'Main session', matchedModels: 5, totalModels: 5 },
				{ classKey: 'subagent:', title: 'Unresolved persona', matchedModels: 1, totalModels: 1 }
			])
		).toBe('Applies to: Main session (5 of 5 models) · Unresolved persona (1 of 1 models)');
	});

	it('excludes a class the rule matched zero models of', () => {
		expect(
			applicabilityLabel([
				{ classKey: 'main:', title: 'Main session', matchedModels: 0, totalModels: 5 }
			])
		).toBe('Applies to: no captured prompt');
	});

	it('reads "no captured prompt" when nothing was sampled anywhere', () => {
		expect(applicabilityLabel([])).toBe('Applies to: no captured prompt');
	});
});

describe('parseReleaseFromBlockText', () => {
	it('extracts the three-segment release from the billing-header text', () => {
		expect(
			parseReleaseFromBlockText('x-anthropic-billing-header: cc_version=2.1.263.des; other=stuff')
		).toBe('2.1.263');
	});

	it('is null when the text carries no cc_version marker', () => {
		expect(parseReleaseFromBlockText('You are Claude Code, an AI coding assistant.')).toBeNull();
	});
});

function versionModel(
	model: string,
	overrides: Partial<PromptClassVersionDto['models'][number]> = {}
): PromptClassVersionDto['models'][number] {
	return {
		model,
		observation_count: 1,
		first_seen: '2026-09-01T00:00:00Z',
		last_seen: '2026-09-07T00:00:00Z',
		latest_request_id: 'req-1',
		latest_session_id: 'sess-1',
		...overrides
	};
}

function versionDto(
	version: string | null,
	models: PromptClassVersionDto['models'],
	overrides: Partial<PromptClassVersionDto> = {}
): PromptClassVersionDto {
	return {
		version,
		builds: 1,
		observation_count: models.reduce((n, m) => n + m.observation_count, 0),
		first_seen: '2026-09-01T00:00:00Z',
		last_seen: '2026-09-07T00:00:00Z',
		models,
		...overrides
	};
}

describe('extractModelReleases', () => {
	const versions: PromptClassVersionDto[] = [
		versionDto('2.1.263', [
			versionModel('claude-fable-5-1', { observation_count: 9, latest_request_id: 'req-263' }),
			versionModel('claude-sonnet-5', { observation_count: 2, latest_request_id: 'req-263-s' })
		]),
		versionDto('2.1.257', [
			versionModel('claude-fable-5-1', { observation_count: 6, latest_request_id: 'req-257' })
		]),
		versionDto(null, [
			versionModel('claude-fable-5-1', { observation_count: 1, latest_request_id: 'req-none' })
		])
	];

	it("filters to one model's releases, in the API's order, marking only the first as current", () => {
		const rows = extractModelReleases(versions, 'claude-fable-5-1');
		expect(rows.map((r) => r.version)).toEqual(['2.1.263', '2.1.257', null]);
		expect(rows.map((r) => r.isCurrent)).toEqual([true, false, false]);
		expect(rows[0]).toMatchObject({ observationCount: 9, latestRequestId: 'req-263' });
	});

	it('excludes releases the model never sent', () => {
		const rows = extractModelReleases(versions, 'claude-sonnet-5');
		expect(rows.map((r) => r.version)).toEqual(['2.1.263']);
		expect(rows[0]).toMatchObject({ isCurrent: true, observationCount: 2 });
	});

	it('is empty for a model with no releases in the list', () => {
		expect(extractModelReleases(versions, 'claude-opus-5')).toEqual([]);
	});
});

describe('splitReleasesForDisplay', () => {
	it('keeps everything visible at or under the cap', () => {
		const result = splitReleasesForDisplay([1, 2, 3], 5);
		expect(result).toEqual({ visible: [1, 2, 3], collapsed: [] });
	});

	it('splits current + next four visible from the rest collapsed', () => {
		const result = splitReleasesForDisplay([1, 2, 3, 4, 5, 6, 7], 5);
		expect(result).toEqual({ visible: [1, 2, 3, 4, 5], collapsed: [6, 7] });
	});
});

describe('buildModelFirstTree', () => {
	function sourceClass(overrides: Partial<ModelFirstSourceClass> = {}): ModelFirstSourceClass {
		return {
			key: 'main:',
			lane: 'main',
			title: 'Main session',
			registrationLabel: null,
			noRoute: false,
			neverObserved: false,
			versionsStatus: 'ready',
			versionsError: null,
			versions: [],
			modelRows: [],
			...overrides
		};
	}

	it("pivots one class into per-model blocks, marking each model's own current release", () => {
		const mainVersions: PromptClassVersionDto[] = [
			versionDto('2.1.263', [
				versionModel('claude-fable-5-1', {
					observation_count: 9,
					latest_request_id: 'req-fable-263'
				}),
				versionModel('claude-sonnet-5', {
					observation_count: 2,
					latest_request_id: 'req-sonnet-263'
				})
			]),
			versionDto('2.1.257', [
				versionModel('claude-fable-5-1', {
					observation_count: 6,
					latest_request_id: 'req-fable-257'
				})
			])
		];
		const classes = [
			sourceClass({
				versions: mainVersions,
				modelRows: [
					{
						model: 'claude-fable-5-1',
						windowedCount: 20,
						lastSeen: '2026-09-07T00:00:00Z',
						sampleBlocks: [{ type: 'text', text: 'fable prompt' }],
						status: 'ready',
						error: null
					},
					{
						model: 'claude-sonnet-5',
						windowedCount: 5,
						lastSeen: '2026-09-06T00:00:00Z',
						sampleBlocks: null,
						status: 'ready',
						error: null
					}
				]
			})
		];

		const tree = buildModelFirstTree(classes);

		expect(tree.models.map((m) => m.model)).toEqual(['claude-fable-5-1', 'claude-sonnet-5']);

		const fable = tree.models[0]!;
		expect(fable.observationCount).toBe(20);
		expect(fable.lastSeen).toBe('2026-09-07T00:00:00Z');
		expect(fable.classes).toHaveLength(1);
		expect(fable.classes[0]!.releases.map((r) => r.version)).toEqual(['2.1.263', '2.1.257']);
		expect(fable.classes[0]!.releases[0]!.isCurrent).toBe(true);
		expect(fable.classes[0]!.currentSampleBlocks).toEqual([{ type: 'text', text: 'fable prompt' }]);

		const sonnet = tree.models[1]!;
		expect(sonnet.classes[0]!.releases.map((r) => r.version)).toEqual(['2.1.263']);
		expect(sonnet.classes[0]!.currentSampleBlocks).toBeNull();
	});

	it('routes a noRoute or never-observed class to the trailing block instead of any model', () => {
		const classes = [
			sourceClass({ key: 'main:steer-author', title: 'steer-author', noRoute: true }),
			sourceClass({
				key: 'subagent:unit-tester',
				title: 'unit-tester',
				lane: 'subagent',
				neverObserved: true,
				registrationLabel: 'unit-tester'
			})
		];

		const tree = buildModelFirstTree(classes);

		expect(tree.models).toEqual([]);
		expect(tree.neverObserved).toEqual([
			{
				classKey: 'main:steer-author',
				lane: 'main',
				title: 'steer-author',
				registrationLabel: null
			},
			{
				classKey: 'subagent:unit-tester',
				lane: 'subagent',
				title: 'unit-tester',
				registrationLabel: 'unit-tester'
			}
		]);
	});

	it('preserves class order (main first, personas alphabetical) within one model, straight from input order', () => {
		const classes = [
			sourceClass({
				key: 'main:',
				title: 'Main session',
				versions: [versionDto('2.1.263', [versionModel('claude-sonnet-5')])],
				modelRows: [
					{
						model: 'claude-sonnet-5',
						windowedCount: 1,
						lastSeen: null,
						sampleBlocks: null,
						status: 'ready',
						error: null
					}
				]
			}),
			sourceClass({
				key: 'subagent:db-dev',
				lane: 'subagent',
				title: 'db-dev',
				versions: [versionDto('2.1.263', [versionModel('claude-sonnet-5')])],
				modelRows: [
					{
						model: 'claude-sonnet-5',
						windowedCount: 1,
						lastSeen: null,
						sampleBlocks: null,
						status: 'ready',
						error: null
					}
				]
			})
		];

		const tree = buildModelFirstTree(classes);
		expect(tree.models).toHaveLength(1);
		expect(tree.models[0]!.classes.map((c) => c.title)).toEqual(['Main session', 'db-dev']);
	});

	it('still groups a class under its known windowed models while its versions fetch is loading', () => {
		const classes = [
			sourceClass({
				versionsStatus: 'loading',
				versions: [],
				modelRows: [
					{
						model: 'claude-fable-5-1',
						windowedCount: 3,
						lastSeen: null,
						sampleBlocks: null,
						status: 'ready',
						error: null
					}
				]
			})
		];

		const tree = buildModelFirstTree(classes);
		expect(tree.models.map((m) => m.model)).toEqual(['claude-fable-5-1']);
		expect(tree.models[0]!.classes[0]!.releases).toEqual([]);
		expect(tree.models[0]!.classes[0]!.versionsStatus).toBe('loading');
	});
});
