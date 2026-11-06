<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent } from 'storybook/test';
	import LegendFilter, { type LegendEntry } from './LegendFilter.svelte';

	const { Story } = defineMeta({
		title: 'Shared/LegendFilter',
		component: LegendFilter,
		tags: ['autodocs']
	});

	const entries: LegendEntry[] = [
		{ key: 'agent', label: 'agent', kind: 'fill', colorToken: '--ident-agent-fg' },
		{
			key: 'toolcall',
			label: 'tool call',
			kind: 'line',
			colorToken: '--ident-agent-fg',
			pattern: '8 3'
		},
		{ key: 'busy', label: 'busy', kind: 'ring', colorToken: '--signal-active' }
	];
</script>

<script lang="ts">
	let active = $state(new Set(['agent', 'toolcall']));
	const onChange = fn();
</script>

<Story
	name="Entries"
	play={async ({ canvas }) => {
		const toolcall = canvas.getByRole('button', { name: 'tool call' });
		const busy = canvas.getByRole('button', { name: 'busy' });

		await expect(toolcall).toHaveAttribute('aria-pressed', 'true');
		await expect(busy).toHaveAttribute('aria-pressed', 'false');
		await expect(getComputedStyle(busy).opacity).toBe('0.32');

		await userEvent.click(busy);
		await expect(busy).toHaveAttribute('aria-pressed', 'true');
		await expect(getComputedStyle(busy).opacity).toBe('1');
		await expect(onChange).toHaveBeenCalled();

		await userEvent.click(toolcall);
		await expect(toolcall).toHaveAttribute('aria-pressed', 'false');
	}}
>
	{#snippet template()}
		<div data-design="funk" style="background: var(--surface-bg); padding: 16px;">
			<LegendFilter {entries} bind:active {onChange} />
		</div>
	{/snippet}
</Story>
