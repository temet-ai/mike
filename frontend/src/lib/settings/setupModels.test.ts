import { describe, expect, it } from 'vitest';
import {
	decisionsModelRow,
	normalizeRef,
	providerOf,
	runningJobs,
	usedBy,
	MODEL_JOBS
} from './setupModels';
import type { FeatureDto, SettingMeta } from '$lib/api/settings';
import type { LlmProvider } from '$lib/api/setup';

const on = (key: string): FeatureDto => ({
	key,
	enabled: true,
	scope: 'session',
	applies: 'new_sessions'
});

describe('setup models', () => {
	it('reads the provider from the model reference', () => {
		expect(providerOf('claude/sonnet')).toBe('claude');
		expect(providerOf('codex/gpt-6-sol')).toBe('codex');
		expect(providerOf('anthropic/claude-sonnet-5')).toBe('openrouter');
	});

	it('adds the provider prefix once', () => {
		expect(normalizeRef('claude', 'sonnet')).toBe('claude/sonnet');
		expect(normalizeRef('claude', 'claude/sonnet')).toBe('claude/sonnet');
		expect(normalizeRef('openrouter', 'google/gemini')).toBe('openrouter/google/gemini');
		expect(normalizeRef('openrouter', 'codex/gpt-6-sol')).toBe('codex/gpt-6-sol');
		expect(normalizeRef('codex', '')).toBe('');
	});

	it('lists only the jobs whose switch is on', () => {
		const jobs = runningJobs([on('steerer'), on('judge_worker')], []);
		expect(jobs.map((j) => j.name)).toEqual(['Steerers', 'Turn judge']);
	});

	it('adds the lesson relations jobs when that setting is on', () => {
		const setting = { key: 'lesson_relations.enabled', value: true } as SettingMeta;
		expect(runningJobs([], [setting]).map((j) => j.key)).toEqual([
			'lesson_relations.proposer_model',
			'lesson_relations.adjudicator_model'
		]);
	});

	it('reports why the decisions model is off and its fixed model when it is on', () => {
		const openrouter = (available: boolean) => ({ id: 'openrouter', available }) as LlmProvider;
		expect(decisionsModelRow([], [], [openrouter(true)]).status).toBe(
			'off: the decisions toggle is off'
		);
		expect(decisionsModelRow([on('decisions')], [], [openrouter(false)]).status).toBe(
			'off: no OpenRouter key'
		);
		expect(decisionsModelRow([on('decisions')], [], [])).toMatchObject({ on: false });
		const running = decisionsModelRow([on('decisions')], [], [openrouter(true)]);
		expect(running).toEqual({ on: true, model: 'typesafe/jev-1.14', status: 'On' });
		const custom = { key: 'decisions.model', value: 'typesafe/jev-2' } as SettingMeta;
		expect(decisionsModelRow([on('decisions')], [custom], [openrouter(true)]).model).toBe(
			'typesafe/jev-2'
		);
	});

	it('says which jobs use a tier', () => {
		const first = MODEL_JOBS.filter((job) => ['Steerers', 'Turn judge'].includes(job.name));
		expect(usedBy(first, 'strong')).toBe('Steerers');
		expect(usedBy(first, 'fast')).toBe('no job you switched on');
	});
});
