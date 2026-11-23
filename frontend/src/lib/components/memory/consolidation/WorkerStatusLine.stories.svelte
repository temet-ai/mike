<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import WorkerStatusLine from './WorkerStatusLine.svelte';
	import { capabilityFor } from '$lib/test-support/__fixtures__/knowledgeConsolidation';

	const { Story } = defineMeta({
		title: 'Components/Memory/WorkerStatusLine',
		component: WorkerStatusLine,
		tags: ['autodocs']
	});
	const RUN = '2026-11-20T16:40:00Z';
	const NOW = Date.parse('2026-11-20T16:43:00Z');
</script>

<Story
	name="States"
	play={async ({ canvas }) => {
		for (const word of ['Off', 'First pass', 'On', 'Degraded', 'Unknown']) {
			await expect(canvas.getAllByText(word).length).toBeGreaterThan(0);
		}
		await expect(canvas.getByText('To fix')).toBeInTheDocument();
		await expect(canvas.getAllByText(/3m ago/).length).toBeGreaterThan(0);
	}}
>
	{#snippet template()}
		<div class="edge mech-group w-[900px]">
			<WorkerStatusLine
				mode="off-never"
				capability={capabilityFor('off-never')}
				lastRun={null}
				now={NOW}
			/>
			<WorkerStatusLine
				mode="off-history"
				capability={capabilityFor('off-history')}
				lastRun={RUN}
				now={NOW}
			/>
			<WorkerStatusLine
				mode="first-pass"
				capability={capabilityFor('first-pass')}
				lastRun={null}
				now={NOW}
			/>
			<WorkerStatusLine
				mode="steady"
				capability={capabilityFor('steady')}
				lastRun={RUN}
				now={NOW}
			/>
			<WorkerStatusLine
				mode="degraded"
				capability={capabilityFor('degraded')}
				lastRun={RUN}
				now={NOW}
			/>
			<WorkerStatusLine mode="unknown" capability={null} lastRun={null} now={NOW} />
		</div>
	{/snippet}
</Story>
