<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, expect } from 'storybook/test';
	import DisclosureMarker from './DisclosureMarker.svelte';

	const { Story } = defineMeta({
		title: 'Shared/DisclosureMarker',
		component: DisclosureMarker,
		tags: ['autodocs'],
		argTypes: {
			open: { control: 'boolean' },
			size: { control: { type: 'number' } }
		}
	});
</script>

<script lang="ts">
	let open = $state(false);
</script>

<Story
	name="InRowHeader"
	args={{ open: false, size: 10 }}
	play={async ({ canvas }) => {
		const header = canvas.getByRole('button', { name: /request headers/i });
		await expect(header).toHaveAttribute('aria-expanded', 'false');
		await userEvent.click(canvas.getByText('5 keys'));
		await expect(header).toHaveAttribute('aria-expanded', 'true');
		await userEvent.click(canvas.getByText('Request headers'));
		await expect(header).toHaveAttribute('aria-expanded', 'false');
	}}
>
	{#snippet template(args)}
		<div class="w-[420px] border border-[var(--border-default)] bg-[var(--surface-card)]">
			<button
				type="button"
				class="flex w-full items-center gap-2.5 px-3 py-2 text-left"
				style="background: {open ? 'var(--surface-2)' : 'transparent'};"
				aria-expanded={open}
				aria-controls="dm-demo-body"
				onclick={() => (open = !open)}
			>
				<DisclosureMarker {open} size={args.size} />
				<span class="text-xs text-[var(--text-2)]">Request headers</span>
				<span class="ml-auto font-mono text-[10px] text-[var(--text-3)]">5 keys</span>
			</button>
			<div
				id="dm-demo-body"
				class="grid transition-[grid-template-rows] duration-200"
				style="grid-template-rows: {open ? '1fr' : '0fr'};"
			>
				<div class="overflow-hidden">
					<pre
						class="m-0 p-3 font-mono text-[11px] text-[var(--text-1)]">accept: text/event-stream</pre>
				</div>
			</div>
		</div>
	{/snippet}
</Story>
