<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import RuleOutcomeTable from './RuleOutcomeTable.svelte';
	import { CODE_CATALOGUE, CODE_RULE_COUNTS } from '$lib/test-support/codeFindingsFixtures';

	const { Story } = defineMeta({
		title: 'CodeFindings/RuleOutcomeTable',
		component: RuleOutcomeTable,
		tags: ['autodocs']
	});
</script>

<Story
	name="Rules that fired"
	args={{ rules: CODE_RULE_COUNTS, catalogue: CODE_CATALOGUE }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const table = canvas.getByRole('table', { name: 'Rules that fired' });
		const rows = within(table).getAllByRole('row');
		await expect(rows).toHaveLength(6);
		const body = rows.slice(1, 5).map((row) => within(row).getAllByRole('cell')[0]!.textContent);
		await expect(body[0]).toContain('rust-no-comments');
		await expect(body[3]).toContain('project-no-println');
		await expect(body[3]).toContain('project');
		const rustTempDir = canvasElement.querySelector('[data-rule="rust-temp-dir"]')!;
		const cells = within(rustTempDir as HTMLElement).getAllByRole('cell');
		await expect(cells[9]).toHaveTextContent('—');
		await expect(cells[10]).toHaveTextContent('—');
		const rustNoComments = within(
			canvasElement.querySelector('[data-rule="rust-no-comments"]') as HTMLElement
		).getAllByRole('cell');
		await expect(rustNoComments[9]).toHaveTextContent('67%');
		await expect(rustNoComments[10]).toHaveTextContent('1m 35s');
		const all = within(
			canvasElement.querySelector('[data-rule="all"]') as HTMLElement
		).getAllByRole('cell');
		await expect(all[0]).toHaveTextContent('All rules');
		await expect(all[1]).toHaveTextContent('171');
		await expect(all[9]).toHaveTextContent('67%');
		await expect(canvas.getAllByRole('columnheader')).toHaveLength(11);
	}}
>
	{#snippet template(args)}
		<div style="max-width: 1100px; padding: 12px;">
			<RuleOutcomeTable {...args} />
		</div>
	{/snippet}
</Story>
