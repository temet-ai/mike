<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import ConsolidationSection from './ConsolidationSection.svelte';
	import {
		PROJECTS,
		stubConsolidationFetch,
		type ConsolidationScenario
	} from '$lib/test-support/__fixtures__/knowledgeConsolidation';

	const { Story } = defineMeta({
		title: 'Components/Memory/ConsolidationSection',
		component: ConsolidationSection,
		tags: ['autodocs']
	});

	const stub = (scenario: ConsolidationScenario) => async () => {
		stubConsolidationFetch(scenario);
		return {};
	};
</script>

{#snippet frame()}
	<div class="w-[1000px] bg-[var(--surface-bg)] p-4">
		<ConsolidationSection projects={PROJECTS} initialProject="3" poll={false} />
	</div>
{/snippet}

<Story
	name="Steady"
	loaders={[stub('steady')]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(
			await canvas.findByText('Ranking Wright problems', { selector: 'a' })
		).toBeInTheDocument();
		await expect(canvas.getAllByRole('group', { name: /measures by run/i })).toHaveLength(1);
		await expect(await canvas.findByText('Last run', { exact: false })).toBeInTheDocument();

		const rows = () => canvasElement.querySelectorAll('[data-slot="change-row"]');
		const undo = (await canvas.findAllByRole('button', { name: /^undo$/i }))[0]!;
		const before = rows().length;
		await userEvent.click(undo);
		await waitFor(() => expect(canvas.getAllByText('Undone').length).toBeGreaterThan(1));
		await expect(rows().length).toBe(before);

		await userEvent.click(canvas.getAllByRole('button', { name: /^keep this one$/i })[0]!);
		await waitFor(() => expect(canvas.getByText('Settled')).toBeInTheDocument());
		await expect(canvas.getByText(/^Kept Lesson 2733\./)).toBeInTheDocument();
	}}
>
	{#snippet template()}{@render frame()}{/snippet}
</Story>

<Story
	name="OffNeverRun"
	loaders={[stub('off-never')]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(
			await canvas.findByText(/sends their text to the consolidation model/i)
		).toBeInTheDocument();
		await expect(
			canvas.getByRole('link', { name: /turn on in workers settings/i })
		).toHaveAttribute('href', '/settings/workers#worker-knowledge_consolidation');
		await expect(canvas.queryByText('Open conflicts')).not.toBeInTheDocument();
	}}
>
	{#snippet template()}{@render frame()}{/snippet}
</Story>

<Story
	name="OffWithHistory"
	loaders={[stub('off-history')]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(await canvas.findByText(/from its last run/)).toBeInTheDocument();
		await expect(await canvas.findByText('Recent changes')).toBeInTheDocument();
	}}
>
	{#snippet template()}{@render frame()}{/snippet}
</Story>

<Story
	name="FirstPass"
	loaders={[stub('first-pass')]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(await canvas.findByText('First pass')).toBeInTheDocument();
		await expect(canvas.getByText(/14 files waiting to be split/)).toBeInTheDocument();
		await expect(canvas.getByText(/The figures appear when that run finishes/)).toBeInTheDocument();
	}}
>
	{#snippet template()}{@render frame()}{/snippet}
</Story>

<Story
	name="Degraded"
	loaders={[stub('degraded')]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(await canvas.findByText('Degraded')).toBeInTheDocument();
		await expect(canvas.getByText('To fix')).toBeInTheDocument();
		await expect(await canvas.findByText('Recent changes')).toBeInTheDocument();
	}}
>
	{#snippet template()}{@render frame()}{/snippet}
</Story>

<Story
	name="EmptyProject"
	loaders={[stub('empty')]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(
			await canvas.findByText(/This project has no lessons or guidance files yet/)
		).toBeInTheDocument();
		await expect(canvas.getByRole('link', { name: /knowledge sources/i })).toBeInTheDocument();
		await expect(canvas.queryByText('Recent changes')).not.toBeInTheDocument();
	}}
>
	{#snippet template()}{@render frame()}{/snippet}
</Story>
