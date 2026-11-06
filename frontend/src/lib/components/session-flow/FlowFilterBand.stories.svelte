<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent } from 'storybook/test';
	import FlowFilterBand from './FlowFilterBand.svelte';
	import { defaultLegend } from '$lib/session-flow/legendEntries';

	const { Story } = defineMeta({
		title: 'Sessions/SessionFlow/FlowFilterBand',
		component: FlowFilterBand,
		parameters: { layout: 'padded' }
	});
</script>

<script lang="ts">
	let active = $state(defaultLegend());
	const onChange = fn();
</script>

<Story
	name="Filter"
	play={async ({ canvas }) => {
		await expect(canvas.getByText('Filter')).toBeVisible();
		await expect(
			canvas.getByText(
				'EVERY ENTRY IS A TOGGLE · DRAG ON THE TIMELINE TO SELECT A RANGE · SELECTING AN ITEM FOCUSES IT'
			)
		).toBeVisible();

		const agentEntry = canvas.getByRole('button', { name: 'AGENT' });
		await expect(agentEntry).toHaveAttribute('aria-pressed', 'true');

		await userEvent.click(agentEntry);
		await expect(agentEntry).toHaveAttribute('aria-pressed', 'false');
		await expect(onChange).toHaveBeenCalled();
		const emitted = onChange.mock.calls.at(-1)![0] as Set<string>;
		await expect(emitted.has('who-agent')).toBe(false);

		await userEvent.click(agentEntry);
		await expect(agentEntry).toHaveAttribute('aria-pressed', 'true');
	}}
>
	{#snippet template()}
		<div data-design="funk" style="background: var(--surface-bg); padding: 16px; max-width: 820px;">
			<FlowFilterBand bind:active {onChange} />
		</div>
	{/snippet}
</Story>
