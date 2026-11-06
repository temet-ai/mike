import type { LlmProvider } from '$lib/api/setup';
import type { SettingMeta } from '$lib/api/settings';
import type { FeatureDto } from '$lib/api/settings';

export type Tier = 'fast' | 'standard' | 'strong';

export const TIERS: readonly Tier[] = ['fast', 'standard', 'strong'];

export const PER_JOB = 'per_role';

export const PROVIDER_ORDER = ['claude', 'codex', 'openrouter'] as const;

export interface ModelJob {
	name: string;
	key: string;
	tier: Tier;
	needs: { switch: string } | { setting: string };
}

export const MODEL_JOBS: readonly ModelJob[] = [
	{ name: 'Steerers', key: 'steerer.model', tier: 'strong', needs: { switch: 'steerer' } },
	{
		name: 'Turn judge',
		key: 'judge_worker.model',
		tier: 'standard',
		needs: { switch: 'judge_worker' }
	},
	{
		name: 'Session judge',
		key: 'session_judge.model',
		tier: 'fast',
		needs: { switch: 'session_judge' }
	},
	{
		name: 'Lesson proposer',
		key: 'judge_lessons.model',
		tier: 'fast',
		needs: { switch: 'judge_lessons' }
	},
	{
		name: 'Lesson relations (proposer)',
		key: 'lesson_relations.proposer_model',
		tier: 'strong',
		needs: { setting: 'lesson_relations.enabled' }
	},
	{
		name: 'Lesson relations (adjudicator)',
		key: 'lesson_relations.adjudicator_model',
		tier: 'fast',
		needs: { setting: 'lesson_relations.enabled' }
	}
];

export function providerOf(ref: string): string {
	if (ref.startsWith('claude/')) return 'claude';
	if (ref.startsWith('codex/')) return 'codex';
	return 'openrouter';
}

export function normalizeRef(provider: string, text: string): string {
	const value = text.trim();
	if (value === '') return '';
	if (provider === 'claude' && !value.startsWith('claude/')) return `claude/${value}`;
	if (provider === 'codex' && !value.startsWith('codex/')) return `codex/${value}`;
	if (provider === 'openrouter') {
		if (value.startsWith('claude/') || value.startsWith('codex/')) return value;
		if (!value.startsWith('openrouter/')) return `openrouter/${value}`;
	}
	return value;
}

export function runningJobs(features: FeatureDto[], settings: SettingMeta[]): ModelJob[] {
	return MODEL_JOBS.filter((job) => {
		if ('switch' in job.needs) {
			const key = job.needs.switch;
			return features.some((f) => f.key === key && f.enabled);
		}
		const key = job.needs.setting;
		return settings.some((s) => s.key === key && s.value === true);
	});
}

export function modelRefs(provider: LlmProvider | undefined): string[] {
	if (!provider) return [];
	const good = provider.models.filter((m) => m.known_good === true).map((m) => m.ref);
	const rest = provider.models.filter((m) => m.known_good !== true).map((m) => m.ref);
	return [...good, ...rest];
}

export function tierDefault(
	provider: LlmProvider | undefined,
	settings: SettingMeta[],
	tier: Tier
): string {
	if (!provider) return '';
	const stored = settings.find(
		(s) => s.key === `llm.provider_models.${provider.id}.${tier}`
	)?.value;
	if (typeof stored === 'string' && stored.trim() !== '') return stored.trim();
	return modelRefs(provider)[0] ?? '';
}

export function usedBy(jobs: ModelJob[], tier: Tier): string {
	const names = jobs.filter((job) => job.tier === tier).map((job) => job.name);
	return names.length === 0 ? 'no job you switched on' : names.join(', ');
}
