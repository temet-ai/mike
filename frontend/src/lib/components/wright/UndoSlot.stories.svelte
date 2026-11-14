<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import UndoSlot from './UndoSlot.svelte';

	const { Story } = defineMeta({
		title: 'Wright/UndoSlot',
		component: UndoSlot,
		tags: ['autodocs']
	});

	const onUndo = fn();
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const widths = [...canvasElement.querySelectorAll('[data-slot="undo-slot"]')].map(
			(slot) => slot.getBoundingClientRect().width
		);
		await expect(new Set(widths.map((w) => Math.round(w))).size).toBe(1);
		await userEvent.click(canvas.getByRole('button', { name: 'Undo: Ready' }));
		await expect(onUndo).toHaveBeenCalledOnce();
		await expect(canvas.getByRole('button', { name: 'Undo: Working' })).toBeDisabled();
		await expect(canvas.getByText('Undone')).toBeVisible();
		await expect(canvas.getByText("Couldn't undo")).toBeVisible();
		await expect(canvas.getByRole('button', { name: 'Undo: Unpaired' })).toBeDisabled();
	}}
>
	{#snippet template()}
		<div
			style="display: flex; flex-direction: column; gap: 8px; padding: 16px; background: var(--surface-card);"
		>
			<div style="display: flex"><UndoSlot subject="Ready" state="ready" {onUndo} /></div>
			<div style="display: flex"><UndoSlot subject="Working" state="undoing" {onUndo} /></div>
			<div style="display: flex"><UndoSlot subject="Done" state="undone" {onUndo} /></div>
			<div style="display: flex">
				<UndoSlot subject="Blocked" state="failed" reason="The resource changed." {onUndo} />
			</div>
			<div style="display: flex">
				<UndoSlot
					subject="Unpaired"
					state="ready"
					disabledReason="Pair this browser to undo"
					{onUndo}
				/>
			</div>
			<div style="display: flex"><UndoSlot subject="Nothing" state="none" {onUndo} /></div>
		</div>
	{/snippet}
</Story>
