<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
	import SetupModelsPanel from './SetupModelsPanel.svelte';
	import { buildFeatures, buildProviders, buildSettings } from './setupFixtures';

	const onSave = fn(async () => {});

	const { Story } = defineMeta({
		title: 'Routes/Settings/setup/SetupModelsPanel',
		component: SetupModelsPanel,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	args={{
		providers: buildProviders(),
		providersError: null,
		settings: buildSettings(),
		features: buildFeatures(['steerer', 'judge_worker']),
		onSave
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByText('Not available')).toBeVisible();
		await expect(canvas.getByText('Steerers')).toBeVisible();
		const decisionsRow = canvas.getByText('Decisions').closest('[data-slot="setup-row"]');
		await expect(decisionsRow?.textContent).toContain('off: the decisions toggle is off');
		await expect(decisionsRow?.textContent).toContain('typesafe/jev-1.14');
		await expect(decisionsRow?.querySelector('select')).toBeNull();
		await expect(canvas.getByText('Turn judge')).toBeVisible();
		const rowOf = (name: string) =>
			within(canvas.getByText(name).closest('[data-slot="setup-row"]') as HTMLElement);
		await userEvent.selectOptions(rowOf('Steerers').getByLabelText('Provider'), 'codex');
		await waitFor(() => expect(onSave).toHaveBeenCalledWith('steerer.model', 'codex/gpt-6-luna'));
		await userEvent.selectOptions(rowOf('Turn judge').getByLabelText('Model'), 'codex/gpt-6-luna');
		await waitFor(() =>
			expect(onSave).toHaveBeenCalledWith('judge_worker.model', 'codex/gpt-6-luna')
		);
		await expect(
			canvas.getByRole('spinbutton', { name: 'Daily spend limit in US dollars' })
		).toBeVisible();
	}}
>
	{#snippet template(args)}
		<div style="width: 44rem; background: var(--surface-card);">
			<SetupModelsPanel {...args} />
		</div>
	{/snippet}
</Story>
