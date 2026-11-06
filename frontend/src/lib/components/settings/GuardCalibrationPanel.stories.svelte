<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, waitFor } from 'storybook/test';
	import GuardCalibrationPanel from './GuardCalibrationPanel.svelte';
	import { stubFetchJsonRouter, typed, type TypedBody } from '$lib/test-support/storyFetchStubs';
	import type { CalibrationResponse } from '$lib/api/crossSession';

	const DATA: CalibrationResponse = {
		snapshot: {
			settings: { input_mode: 'off', threshold_mode: 'off', calibration_min_labelled: 20 },
			rollup: { id: 31, finished_at: '2026-09-24T03:00:00Z' },
			rows: [
				{ family: 'steer_calibration', question: 'unbacked_wait', scope_kind: 'global', dimension: 'all', dimension_value: 'all', k: 9, n: 10, ci_low: 0.6, ci_high: 0.98 }
			],
			thresholds: [{ id: 5, question: 'unbacked_wait', scope: 'global', applied_threshold: 0.8, computed_at: '2026-09-24T03:05:00Z' }]
		},
		block_if_live: { text: '[calibration cal-v1 as_of=2026-09-24T03:00:00Z window=30d]\n- unbacked_wait, all projects: confirmed 9 of 10.' }
	};

	function router(_method: string, url: string): TypedBody {
		if (url.includes('/api/cross-session/calibration')) return typed<CalibrationResponse>(DATA);
		return typed<Record<string, never>>({});
	}

	const { Story } = defineMeta({
		title: 'Settings/GuardCalibrationPanel',
		component: GuardCalibrationPanel,
		tags: ['autodocs']
	});
</script>

<Story
	name="GuardCalibrationPanel"
	loaders={[
		async () => {
			stubFetchJsonRouter(router);
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		await waitFor(() => expect(canvasElement.querySelector('[data-calibration-row="unbacked_wait:global"]')).not.toBeNull());
		await expect(canvasElement.querySelector('[data-calibration-row]')?.textContent).toContain('confirmed 9 of 10');
		await expect(canvasElement.querySelector('[data-threshold-row="5"]')?.textContent).toContain('0.8');
		await expect(canvasElement.querySelector('[data-calibration-block]')?.textContent).toContain('cal-v1');
	}}
>
	{#snippet template()}
		<div class="bg-[var(--surface-bg)] p-4">
			<GuardCalibrationPanel />
		</div>
	{/snippet}
</Story>
