<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, userEvent } from 'storybook/test';
	import FeatureToggleRow from './FeatureToggleRow.svelte';

	const { Story } = defineMeta({
		title: 'Routes/Settings/detail/FeatureToggleRow',
		component: FeatureToggleRow,
		tags: ['autodocs']
	});
</script>

<script lang="ts">
	let enabled = $state(true);
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const sw = canvas.getByRole('switch');
		await expect(sw.getAttribute('aria-checked')).toBe('true');
		await userEvent.click(sw);
		await expect(sw.getAttribute('aria-checked')).toBe('false');
		await userEvent.click(sw);
		await expect(sw.getAttribute('aria-checked')).toBe('true');
	}}
>
	{#snippet template()}
		<div style="max-width:600px;">
			<FeatureToggleRow
				name="steerer"
				label="steerer"
				description="The steerer fleet evaluates every turn of enrolled sessions against configured drive topics and nudges the session back on track when it drifts."
				{enabled}
				onToggle={(v) => (enabled = v)}
			/>
		</div>
	{/snippet}
</Story>
