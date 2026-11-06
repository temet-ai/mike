<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect } from 'storybook/test';
	import SyncStatusPanel from './SyncStatusPanel.svelte';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import type { SyncStatusDto } from '$lib/api/settings';

	const PINNED_NOW = Date.parse('2026-06-14T15:45:00.000Z');
	const POPULATED: SyncStatusDto = {
		last_run_at: new Date(PINNED_NOW - 18_000).toISOString(),
		dest_reachable: true,
		last_report: {
			tables: [
				['requests.requests', 12],
				['responses.responses', 4]
			],
			total_rows: 16
		},
		last_error:
			'resync worker: batch insert into "responses.responses" failed after 4 rows: ' +
			'pq: duplicate key value violates unique constraint "responses_pkey" (row id=91238)'
	};

	const { Story } = defineMeta({
		title: 'Routes/Settings/detail/SyncStatusPanel',
		component: SyncStatusPanel,
		tags: ['autodocs'],
		parameters: {
			docs: {
				description: {
					component:
						'Read-only panel showing what the local→remote DB resync worker last did: ' +
						'remote-DB reachability, last run time, total rows synced, and the full ' +
						'per-table breakdown. Sits next to the db_resync feature toggle on /settings.'
				}
			}
		},
		argTypes: {
			status: { control: false }
		}
	});
</script>

<Story
	name="Populated"
	tags={['interactive', 'data-driven', 'visual']}
	args={{ status: POPULATED }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(
			canvas.getByText('Remote DB reachable'),
			'reachability indicator should report reachable'
		).toBeInTheDocument();

		await expect(
			canvas.getByText(/16\s+rows synced/),
			'total rows summary must render the count'
		).toBeInTheDocument();

		await expect(
			canvas.getByText('requests.requests'),
			'first table name must render in full'
		).toBeInTheDocument();
		await expect(
			canvas.getByText('responses.responses'),
			'second table name must render in full'
		).toBeInTheDocument();
		const counts = canvasElement.querySelectorAll('.sync-table-count');
		await expect(counts.length, 'both table counts must render').toBe(2);
		await expect(counts[0]?.textContent?.trim()).toBe('12');
		await expect(counts[1]?.textContent?.trim()).toBe('4');

		await expect(
			canvas.queryByText('never run'),
			'a run has happened — the never-run state must NOT show'
		).toBeNull();

		const errorBlock = canvasElement.querySelector('.sync-error');
		await expect(errorBlock, 'error block must render when last_error is set').not.toBeNull();
		await expect(
			within(canvasElement).getByText(
				'resync worker: batch insert into "responses.responses" failed after 4 rows: ' +
					'pq: duplicate key value violates unique constraint "responses_pkey" (row id=91238)'
			),
			'the full error message must render, unclipped'
		).toBeInTheDocument();

		await captureStorySnapshot(canvasElement, 'SyncStatusPanel/Populated');
	}}
>
	{#snippet template(args)}
		<div style="max-width:600px; padding:24px; background:var(--bg);">
			<SyncStatusPanel {...args} now={PINNED_NOW} />
		</div>
	{/snippet}
</Story>
