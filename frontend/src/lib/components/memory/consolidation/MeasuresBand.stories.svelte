<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor } from 'storybook/test';
	import MeasuresBand from './MeasuresBand.svelte';
	import { measures } from '$lib/test-support/__fixtures__/knowledgeConsolidation';

	const { Story } = defineMeta({
		title: 'Components/Memory/MeasuresBand',
		component: MeasuresBand,
		tags: ['autodocs']
	});
	const rows = measures(30);
</script>

<script lang="ts">
	let pointAt = $state<number | null>(null);
</script>

<Story
	name="EightCharts"
	play={async ({ canvasElement, canvas }) => {
		await expect(canvasElement.querySelectorAll('[data-slot="trend-chart"]')).toHaveLength(8);
		await expect(canvas.getByText('Statements in force')).toBeInTheDocument();
		await expect(canvas.getByText(/−132 since .*, better/)).toBeInTheDocument();
		const group = canvas.getByRole('group', { name: /measures by run/i });
		group.focus();
		await waitFor(() => expect(canvasElement.querySelectorAll('.tc__pointer')).toHaveLength(8));
		await userEvent.keyboard('{ArrowLeft}');
		await userEvent.keyboard('{Home}');
		await waitFor(() => expect(pointAt).toBe(Date.parse(rows[0]!.measured_at)));
		await userEvent.keyboard('{Escape}');
		await waitFor(() => expect(canvasElement.querySelectorAll('.tc__pointer')).toHaveLength(0));
	}}
>
	{#snippet template()}
		<div class="edge mech-group w-[1000px]">
			<MeasuresBand measures={rows} bind:pointAt />
		</div>
	{/snippet}
</Story>
