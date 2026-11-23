<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import Page from './+page.svelte';
	import { stubConsolidationFetch } from '$lib/test-support/__fixtures__/knowledgeConsolidation';

	const { Story } = defineMeta({
		title: 'Routes/Memory/Subject',
		component: Page,
		tags: ['autodocs']
	});

	const pageState = (id: string) => ({
		sveltekit_experimental: {
			state: {
				page: { url: new URL(`http://localhost/memory/subjects/${id}?project=3`), params: { id } }
			}
		}
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			stubConsolidationFetch('steady');
			return {};
		}
	]}
	parameters={pageState('41')}
	play={async ({ canvas }) => {
		await expect(
			await canvas.findByRole('heading', { name: 'Ranking Wright problems' })
		).toBeInTheDocument();
		await expect(canvas.getByRole('table', { name: 'Statements' })).toBeVisible();
		await expect((await canvas.findAllByText('In conflict')).length).toBe(2);
		for (const word of ['Covered', 'Superseded', 'Gone']) {
			await expect(canvas.getAllByText(word).length).toBeGreaterThan(0);
		}
		await expect((await canvas.findAllByText('instructions · AGENTS.md')).length).toBeGreaterThan(
			0
		);
		await expect(canvas.getAllByLabelText('What the judge read').length).toBeGreaterThan(0);
		await expect(canvas.getByRole('link', { name: 'Consolidation' })).toHaveAttribute(
			'href',
			'/memory#consolidation'
		);
	}}
>
	{#snippet template()}
		<div class="h-[1100px] w-[1100px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="NotFound"
	loaders={[
		async () => {
			stubConsolidationFetch('steady');
			return {};
		}
	]}
	parameters={pageState('999')}
	play={async ({ canvas }) => {
		await expect(await canvas.findByText('No such subject')).toBeInTheDocument();
		await expect(canvas.getByText(/Subject 999 does not exist/)).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="w-[1100px] bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>
