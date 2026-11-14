<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import RuleCatalogueTable from './RuleCatalogueTable.svelte';
	import { CODE_CATALOGUE } from '$lib/test-support/codeFindingsFixtures';

	const { Story } = defineMeta({
		title: 'CodeFindings/RuleCatalogueTable',
		component: RuleCatalogueTable,
		tags: ['autodocs']
	});
</script>

<Story
	name="Catalogue by language"
	args={{ catalogue: CODE_CATALOGUE }}
	play={async ({ canvas }) => {
		await expect(canvas.getByRole('table', { name: 'Rule catalogue' })).toBeVisible();
		await expect(canvas.getByText('rust · 3')).toBeVisible();
		await expect(canvas.getByText('bash · 1')).toBeVisible();
		const rows = canvas.getAllByRole('row');
		const first = within(rows[2]!).getAllByRole('cell')[0]!;
		await expect(first).toHaveTextContent('project-no-println');
		await expect(
			canvas.getByText('Reviewer comments about fixed /tmp paths in deploy scripts')
		).toBeVisible();
	}}
>
	{#snippet template(args)}
		<div style="max-width: 900px; padding: 12px;">
			<RuleCatalogueTable {...args} />
		</div>
	{/snippet}
</Story>
