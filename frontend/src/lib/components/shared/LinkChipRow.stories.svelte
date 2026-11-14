<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import { CircleAlert, ExternalLink, Hash, MessageSquare } from '@lucide/svelte';
	import LinkChipRow, { type LinkChip } from './LinkChipRow.svelte';

	const { Story } = defineMeta({
		title: 'Shared/LinkChipRow',
		component: LinkChipRow,
		tags: ['autodocs']
	});

	const onOpen = fn();

	const ITEMS: LinkChip[] = [
		{ icon: CircleAlert, kind: 'Problem', text: '707', href: '#problem-707' },
		{
			icon: ExternalLink,
			text: 'Commit afc42e2',
			href: 'https://example.com/commit/afc42e2',
			external: true
		},
		{ icon: MessageSquare, text: 'Open thread', onclick: onOpen },
		{ icon: Hash, text: 'Message 3f9a (not a link)' }
	];
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByRole('link', { name: /Problem 707/ })).toHaveAttribute(
			'href',
			'#problem-707'
		);
		const external = canvas.getByRole('link', { name: /Commit afc42e2/ });
		await expect(external).toHaveAttribute('target', '_blank');
		await userEvent.click(canvas.getByRole('button', { name: /Open thread/ }));
		await expect(onOpen).toHaveBeenCalledOnce();
		await expect(canvas.getByText(/not a link/).closest('a, button')).toBeNull();
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface-card);">
			<LinkChipRow items={ITEMS} label="Links" />
		</div>
	{/snippet}
</Story>
