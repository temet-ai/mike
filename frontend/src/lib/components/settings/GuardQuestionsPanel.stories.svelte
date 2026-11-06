<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, waitFor, within } from 'storybook/test';
	import GuardQuestionsPanel from './GuardQuestionsPanel.svelte';
	import { stubFetchJsonRouter, typed, type TypedBody } from '$lib/test-support/storyFetchStubs';
	import type { GuardMetrics, GuardQuestion, GuardQuestionDto, GuardSummary } from '$lib/api/guard';
	import type { SteerQuestionsResponse } from '$lib/api/steerQuestions';

	const question = (q: GuardQuestion, dependency: string): GuardQuestionDto => ({
		question: q,
		mode: q === 'authorised_handback' ? 'off' : 'shadow',
		confidence_threshold: 0.75,
		floor: 0.6,
		bar: 0.85,
		dependency_state: dependency,
		counts_24h: { rows: 12, would_fire: 2 }
	});

	const SUMMARY: GuardSummary = {
		enabled: true,
		steerer_id: 17,
		steerer_enabled: false,
		rubric_version: 'guard-v1-2026-09',
		gate_version: 'gate-v1',
		steering_facts_version: 1,
		labeller_version: 'guard-labeller-v1',
		labeller: { state: 'ok', since: null, consecutive_failures: 0 },
		settings: { max_calls_per_day: 150, max_label_calls_per_day: 80, recall_sample_pct: 10 },
		questions: [
			question('relapse', 'waiting on live reaction verdicts (JC12)'),
			question('unbacked_wait', 'wait backing from steering facts (JC15)'),
			question('authorised_handback', 'directives from the dossier (JC03)'),
			question('unobserved_done', 'observation after change from steering facts (JC15)')
		]
	};

	const METRICS: GuardMetrics = {
		since: '2026-09-11T00:00:00Z',
		until: '2026-09-25T00:00:00Z',
		metrics: [
			{
				question: 'unbacked_wait',
				trigger_point: 'turn_end',
				funnel: { rows: 40, would_fire: 5, labelled: 30 },
				precision: { k: 4, n: 5, value: 0.8, low: 0.38, high: 0.96 },
				recall: 0.5,
				recall_ci: [0.2, 0.8],
				first_vote_precision: { k: 4, n: 6, value: 0.67, low: 0.3, high: 0.9 },
				reach_share: { k: 0, n: 5, value: 0, low: 0, high: 0.43 },
				bar: 0.9,
				daily_cap_excluded: 0
			}
		],
		audit: { audit_set_sessions: 9, audit_set_fires: 0, offending_evaluations: [] }
	};

	const STEER: SteerQuestionsResponse = {
		steerer_id: 17,
		questions: [
			{
				question: 'relapse',
				policy: { id: 1, steerer_id: 17, mode: 'shadow', holdback_pct: null },
				effective_holdback_pct: 10,
				bar: { min_n: 40, min_precision: 0.85, targets: ['append', 'wake'] },
				gate: {
					append: { passes: false, reasons: ['labelled 12 < 40'] },
					wake: { passes: false, reasons: ['labelled 12 < 40'] }
				},
				live: { arms: { treated: 0, holdback: 0 } },
				effect: { status: 'monitoring', primary: { rd: null, ci: null } },
				promotions: []
			}
		]
	};

	function router(method: string, url: string): TypedBody {
		if (url.includes('/api/steering/questions')) return typed<SteerQuestionsResponse>(STEER);
		if (url.includes('/api/steering/guard/metrics')) return typed<GuardMetrics>(METRICS);
		if (url.includes('/api/steering/guard')) return typed<GuardSummary>(SUMMARY);
		return typed<Record<string, never>>({});
	}

	const { Story } = defineMeta({
		title: 'Settings/GuardQuestionsPanel',
		component: GuardQuestionsPanel,
		tags: ['autodocs']
	});
</script>

<Story
	name="GuardQuestionsPanel"
	loaders={[
		async () => {
			stubFetchJsonRouter(router);
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await waitFor(() => expect(canvasElement.querySelectorAll('[data-guard-question]').length).toBe(4));
		await expect(canvas.getByText('waiting on live reaction verdicts (JC12)')).toBeTruthy();
		const wakeOption = canvasElement.querySelector('[data-guard-question="relapse"] option[value="wake"]');
		await expect(wakeOption?.hasAttribute('disabled')).toBe(true);
		await waitFor(() =>
			expect(canvasElement.querySelector('[data-guard-audit]')?.textContent ?? '').toContain('0 would-fires')
		);
		await expect(canvasElement.querySelector('[data-guard-metric="unbacked_wait:turn_end"]')?.textContent ?? '').toContain(
			'80%'
		);
		await waitFor(() => expect(canvasElement.querySelector('[data-guard-promotion="relapse"]')).not.toBeNull());
		const promote = [...canvasElement.querySelectorAll('[data-guard-promotion="relapse"] button')].find((b) =>
			b.textContent?.includes('Promote to wake')
		);
		await expect(promote?.hasAttribute('disabled')).toBe(true);
	}}
>
	{#snippet template()}
		<div class="bg-[var(--surface-bg)] p-4">
			<GuardQuestionsPanel />
		</div>
	{/snippet}
</Story>
