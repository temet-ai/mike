<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent } from 'storybook/test';
	import { Root as DataTable, Row, Cell, GroupRow } from './index';
	import type { DataTableColumn } from './index';

	const { Story } = defineMeta({
		title: 'UI/DataTable',
		component: DataTable,
		tags: ['autodocs']
	});

	const COLUMNS: DataTableColumn[] = [
		{ key: 'rule', label: 'Rule' },
		{ key: 'found', label: 'Found', align: 'end', tooltip: 'Findings edits added.' },
		{ key: 'open', label: 'Open', align: 'end' },
		{ key: 'replaces', label: 'Replaces' }
	];
</script>

<Story
	name="Composed"
	play={async ({ canvas, canvasElement }) => {
		const table = canvas.getByRole('table', { name: 'Rules' });
		await expect(table).toBeVisible();
		await expect(canvas.getAllByRole('columnheader')).toHaveLength(4);
		await expect(canvas.getByRole('rowheader', { name: 'rust · 2' })).toBeVisible();
		const row = canvasElement.querySelector<HTMLElement>('[data-slot="data-table-row"]')!;
		const before = row.getBoundingClientRect().height;
		await userEvent.hover(row);
		await expect(row.getBoundingClientRect().height).toBe(before);
		const found = canvas.getByText('120');
		await expect(found).toHaveAttribute('data-align', 'end');
		const long = canvas.getByText(/pre-commit hook that greps/);
		await expect(long.scrollWidth).toBeLessThanOrEqual(long.clientWidth + 1);
	}}
>
	{#snippet template()}
		<div class="w-[560px]">
			<DataTable label="Rules" columns={COLUMNS}>
				<tbody>
					<GroupRow colspan={4} label="rust · 2" />
					<Row>
						<Cell mono>rust-no-comments</Cell>
						<Cell align="end" mono>120</Cell>
						<Cell align="end" mono tone="quiet">0</Cell>
						<Cell tone="quiet">—</Cell>
					</Row>
					<Row>
						<Cell mono>rust-temp-dir</Cell>
						<Cell align="end" mono>7</Cell>
						<Cell align="end" mono>3</Cell>
						<Cell>
							A pre-commit hook that greps for hand-rolled temporary directories and rejects the
							commit with a message naming the helper to use instead.
						</Cell>
					</Row>
				</tbody>
				<tfoot>
					<Row>
						<Cell>All rules</Cell>
						<Cell align="end" mono>127</Cell>
						<Cell align="end" mono>3</Cell>
						<Cell tone="quiet">—</Cell>
					</Row>
				</tfoot>
			</DataTable>
		</div>
	{/snippet}
</Story>
