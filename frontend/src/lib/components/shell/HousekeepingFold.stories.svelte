<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, userEvent, fn } from 'storybook/test';
	import HousekeepingFold from './HousekeepingFold.svelte';
	import type { HousekeepingItemView } from './timelineTurnView';

	const ITEMS: HousekeepingItemView[] = [
		{ id: 41, requestNumber: 41, label: 'quota probe', toolName: null },
		{ id: 42, requestNumber: 42, label: 'title sidecar', toolName: null },
		{ id: 43, requestNumber: 43, label: 'monitor', toolName: null }
	];

	const { Story } = defineMeta({
		title: 'Shell/HousekeepingFold',
		component: HousekeepingFold,
		tags: ['autodocs'],
		args: { anchorId: 41, items: ITEMS, onToggle: fn(), onOpenRequest: fn() }
	});
</script>

<script lang="ts">
	let open = $state(false);
</script>

<Story
	name="Default"
	play={async ({ canvasElement, args }) => {
		const canvas = within(canvasElement);
		const toggle = canvas.getByRole('button', { name: /expand 3 housekeeping requests/i });
		await expect(toggle).toHaveTextContent(/quota probe, title sidecar, monitor/i);

		await userEvent.click(toggle);
		await expect(args.onToggle).toHaveBeenCalled();
		await expect(canvas.getByText('title sidecar')).toBeInTheDocument();

		await userEvent.click(canvas.getByRole('button', { name: /#42 title sidecar/i }));
		await expect(args.onOpenRequest).toHaveBeenCalledWith(42);
	}}
>
	{#snippet template(args)}
		<div style="width: 620px; background: var(--surface-bg); padding: 12px;">
			<HousekeepingFold
				anchorId={args.anchorId}
				items={args.items}
				{open}
				onToggle={() => {
					open = !open;
					args.onToggle();
				}}
				onOpenRequest={args.onOpenRequest}
			/>
		</div>
	{/snippet}
</Story>
