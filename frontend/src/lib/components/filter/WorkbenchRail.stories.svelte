<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import WorkbenchRail from './WorkbenchRail.svelte';

	const { Story } = defineMeta({
		title: 'Routes/Filter/WorkbenchRail',
		component: WorkbenchRail,
		tags: ['autodocs']
	});
</script>

<script lang="ts">
	let expanded = $state(false);
	let streaming = $state(true);
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const rail = await canvas.findByRole('button', { name: 'Expand Prompt assistant' });
		await expect(rail).toHaveAttribute('aria-expanded', 'false');
		await expect(await canvas.findByText('Prompt assistant')).toBeInTheDocument();

		await userEvent.click(rail);
		await expect(expanded).toBe(true);
	}}
>
	{#snippet template()}
		<div style="display: flex; height: 480px; background: var(--surface-bg);">
			<WorkbenchRail
				label="Prompt assistant"
				{streaming}
				onExpand={() => (expanded = true)}
			/>
		</div>
	{/snippet}
</Story>
