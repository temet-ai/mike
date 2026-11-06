<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect } from 'storybook/test';
	import RunsStrip from './RunsStrip.svelte';
	import type { ConsolidationRun } from '$lib/api/memoryDecisions';

	const { Story } = defineMeta({
		title: 'Memory/RunsStrip',
		component: RunsStrip,
		tags: ['autodocs']
	});

	const NOW = new Date('2026-07-29T14:00:00Z').getTime();

	const RUNS: ConsolidationRun[] = [
		{
			id: 3,
			uuid: '018f7000-0000-7000-8000-000000000003',
			kind: 'session',
			trigger: 'schedule',
			status: 'completed',
			started_at: '2026-07-29T13:40:00Z',
			finished_at: '2026-07-29T13:40:04Z',
			error: null,
			action_count: 2
		},
		{
			id: 2,
			uuid: '018f7000-0000-7000-8000-000000000002',
			kind: 'nightly',
			trigger: 'schedule',
			status: 'completed',
			started_at: '2026-07-29T03:00:00Z',
			finished_at: '2026-07-29T03:00:41Z',
			error: null,
			action_count: 5
		},
		{
			id: 1,
			uuid: '018f7000-0000-7000-8000-000000000001',
			kind: 'gc',
			trigger: 'schedule',
			status: 'partial',
			started_at: '2026-07-27T03:05:00Z',
			finished_at: '2026-07-27T03:05:12Z',
			error: 'embedding service unavailable for 1 of 4 candidates',
			action_count: 3
		}
	];
</script>

<Story
	name="Default"
	args={{ runs: RUNS, nightlyHourUtc: 3, now: NOW }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByText('Last session run')).toBeInTheDocument();
		await expect(canvas.getByText('Last nightly run')).toBeInTheDocument();
		await expect(canvas.getByText('Last GC pass')).toBeInTheDocument();
		await expect(canvas.getAllByText('completed')).toHaveLength(2);
		await expect(canvas.getByText('partial')).toBeInTheDocument();
		await expect(canvas.getByText('Next nightly (expected)')).toBeInTheDocument();
	}}
>
	{#snippet template(args)}
		<div style="max-width: 900px; padding: 12px;">
			<RunsStrip {...args} />
		</div>
	{/snippet}
</Story>
