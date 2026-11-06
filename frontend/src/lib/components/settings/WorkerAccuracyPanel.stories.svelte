<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import WorkerAccuracyPanel from './WorkerAccuracyPanel.svelte';
	import type { EvalRateDto } from '$lib/api/evalMetrics';

	function rate(over: Partial<EvalRateDto>): EvalRateDto {
		return {
			id: 1,
			rollup_id: 7,
			for_day: '2026-09-24',
			family: 'accuracy',
			metric: 'precision',
			population: 'real_turns',
			scope_kind: 'global',
			project_id: null,
			window_start: null,
			window_end: '2026-09-24T00:00:00Z',
			window_days: null,
			role: 'turn_judge',
			bundle_id: null,
			bundle_label: 'legacy:google/gemini-3-flash-preview:r3-2026-08-29',
			axis: 'legacy_outcome',
			class: 'corrected',
			label_tier: 'seed',
			label_split: 'dev',
			label_source: 'fleet_mirror/ground_truth',
			dimension: null,
			dimension_value: null,
			steerer_id: null,
			question: null,
			mode: null,
			k: 14,
			n: 25,
			value: 0.56,
			ci_low: 0.3707,
			ci_high: 0.7333,
			weighted: false,
			n_effective: null,
			detail: {},
			...over
		};
	}

	const RATES: EvalRateDto[] = [
		rate({ id: 1 }),
		rate({ id: 2, metric: 'recall', k: 14, n: 52, value: 0.0682, weighted: true, n_effective: 31.4, ci_low: 0.02, ci_high: 0.19 })
	];
	const MIX: EvalRateDto[] = [
		rate({ id: 3, family: 'label_mix', metric: 'share', window_days: 7, label_tier: null, class: 'corrected', k: 6, n: 190, value: 0.0316 }),
		rate({ id: 4, family: 'label_mix', metric: 'share', window_days: 28, label_tier: null, class: 'corrected', k: 150, n: 760, value: 0.197 }),
		rate({ id: 5, family: 'label_mix', metric: 'share', window_days: 7, population: 'replied_real_turns', class: 'corrected', k: 4, n: 120, value: 0.033 })
	];

	const { Story } = defineMeta({
		title: 'Settings/WorkerAccuracyPanel',
		component: WorkerAccuracyPanel,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	args={{ role: 'turn_judge', rates: RATES, mix: MIX, rollup: null }}
	play={async ({ canvasElement }) => {
		await expect(canvasElement.textContent).toContain('56.0% (14/25)');
		await expect(canvasElement.textContent).toContain('n_eff ≈ 31.4');
		await expect(canvasElement.textContent).toContain('32-49% band');
	}}
/>
