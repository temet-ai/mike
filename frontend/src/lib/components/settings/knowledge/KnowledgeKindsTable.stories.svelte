<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import KnowledgeKindsTable from './KnowledgeKindsTable.svelte';
	import { knowledgeKindsFixture } from '../../__fixtures__/knowledgeKinds';

	const { Story } = defineMeta({
		title: 'Components/Settings/KnowledgeKindsTable',
		component: KnowledgeKindsTable,
		tags: ['autodocs']
	});

	let kinds = $state(knowledgeKindsFixture());
	let toggled = $state('');
	let tuned = $state('');
	let deleted = $state('');

	function swapCounts(): void {
		kinds = kinds.map((k) => ({
			...k,
			stats: {
				...k.stats,
				items: k.stats.items + 1000,
				embedded: k.stats.embedded + 999,
				waiting: k.stats.waiting + 1
			}
		}));
	}

	function rowRects(canvasElement: HTMLElement): { top: number; height: number }[] {
		return [
			...canvasElement.querySelectorAll<HTMLElement>('tbody [data-slot="data-table-row"]')
		].map((r) => {
			const box = r.getBoundingClientRect();
			return { top: box.top, height: box.height };
		});
	}
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			kinds = knowledgeKindsFixture();
			toggled = tuned = deleted = '';
			return {};
		}
	]}
	play={async ({ canvas, canvasElement }) => {
		const rows = canvasElement.querySelectorAll('tbody [data-slot="data-table-row"]');
		await expect(rows).toHaveLength(4);
		const order = [...rows].map((r) => r.querySelector('.kk-sub')?.textContent);
		await expect(order).toEqual(['artifact', 'invoice', 'lesson', 'turn']);

		// Built-in kinds cannot be deleted; the custom one can.
		await expect(canvas.queryByRole('button', { name: 'Delete Artifacts' })).toBeNull();
		await expect(canvas.getByRole('button', { name: 'Delete Invoices' })).toBeVisible();

		// Tune lines up across rows.
		const tunes = canvas.getAllByRole('button', { name: /^Tune/ });
		await expect(tunes).toHaveLength(4);
		const left = tunes[0]?.getBoundingClientRect().left;
		for (const t of tunes) await expect(t.getBoundingClientRect().left).toBe(left);

		// New counts repaint in place: same order, tops and heights.
		const before = rowRects(canvasElement);
		await userEvent.click(canvas.getByRole('button', { name: 'Swap counts' }));
		await waitFor(() => expect(canvas.getByText('13,840')).toBeVisible());
		await expect(rowRects(canvasElement)).toEqual(before);

		// The off kind reads off.
		await expect(canvas.getByRole('switch', { name: 'Embed Lessons' })).toHaveAttribute(
			'aria-checked',
			'false'
		);
		await userEvent.click(canvas.getByRole('switch', { name: 'Embed Lessons' }));
		await expect(toggled).toBe('lesson:true');
		await userEvent.click(canvas.getByRole('button', { name: 'Tune Invoices' }));
		await expect(tuned).toBe('invoice');
		await userEvent.click(canvas.getByRole('button', { name: 'Delete Invoices' }));
		await expect(deleted).toBe('invoice');

		// Failed: disabled with no errors, opens the whole error otherwise.
		const none = canvas.getByRole('button', { name: 'Read the newest error for Lessons' });
		await expect(none).toBeDisabled();
		await userEvent.click(
			canvas.getByRole('button', { name: 'Read the newest error for Artifacts' })
		);
		const dialog = await within(document.body).findByRole('dialog');
		await expect(dialog).toHaveTextContent('Newest error · Artifacts');
		await expect(dialog).toHaveTextContent('model "embeddinggemma" is not loaded');
		await expect(dialog).toHaveTextContent('retry after the server reports ready');
	}}
>
	{#snippet template()}
		<div class="w-[1100px] bg-[var(--surface-card)]">
			<button type="button" onclick={swapCounts}>Swap counts</button>
			<KnowledgeKindsTable
				{kinds}
				onToggle={(kind, enabled) => (toggled = `${kind}:${enabled}`)}
				onTune={(kind) => (tuned = kind)}
				onDelete={(kind) => (deleted = kind)}
			/>
		</div>
	{/snippet}
</Story>
