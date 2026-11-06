<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
	import SetupFeaturesPanel from './SetupFeaturesPanel.svelte';
	import { buildFeatures, buildPresets } from './setupFixtures';

	const onApply = fn(async () => 5);

	const { Story } = defineMeta({
		title: 'Routes/Settings/setup/SetupFeaturesPanel',
		component: SetupFeaturesPanel,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	args={{
		presets: buildPresets(),
		features: buildFeatures(['recall_injector', 'agent_lessons', 'reminders', 'steerer']),
		onApply
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByText('Custom')).toBeVisible();
		await userEvent.click(canvas.getByRole('radio', { name: /^Watch only/ }));
		await expect(canvas.getByText(/turns off these switches that are on now/)).toHaveTextContent(
			'recall_injector, agent_lessons, reminders, steerer'
		);
		await userEvent.click(canvas.getByRole('button', { name: /Apply/ }));
		await waitFor(() => expect(onApply).toHaveBeenCalledOnce());
		await expect(onApply).toHaveBeenCalledWith(expect.objectContaining({ id: 'watch-only' }));
		await waitFor(() =>
			expect(canvas.getByRole('status')).toHaveTextContent('Changed 5 switches.')
		);
	}}
>
	{#snippet template(args)}
		<div style="width: 40rem; background: var(--surface-card);">
			<SetupFeaturesPanel {...args} />
		</div>
	{/snippet}
</Story>
