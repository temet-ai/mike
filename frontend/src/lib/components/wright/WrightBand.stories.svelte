<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent } from 'storybook/test';
	import WrightBand from './WrightBand.svelte';

	const { Story } = defineMeta({
		title: 'Wright/WrightBand',
		component: WrightBand,
		tags: ['autodocs']
	});

	const onShowHeld = fn();
</script>

<Story
	name="Default"
	play={async ({ canvas }) => {
		const header = canvas.getByRole('button', { name: /Open now/ });
		await expect(header).toHaveAttribute('aria-expanded', 'true');
		await userEvent.click(header);
		await expect(header).toHaveAttribute('aria-expanded', 'false');
		await userEvent.click(header);
		await expect(header).toHaveAttribute('aria-expanded', 'true');
		await userEvent.click(canvas.getByRole('button', { name: '2 new' }));
		await expect(onShowHeld).toHaveBeenCalledOnce();
	}}
>
	{#snippet template()}
		<div
			class="edge mech-group"
			style="--edge-line: var(--border-subtle); --edge-surface: var(--surface-card);"
		>
			<WrightBand id="story-open" label="Open now" meta="3 open" held={2} {onShowHeld}>
				<p>Three rows would be here.</p>
			</WrightBand>
		</div>
	{/snippet}
</Story>
