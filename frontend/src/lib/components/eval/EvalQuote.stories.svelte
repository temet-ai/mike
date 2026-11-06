<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import EvalQuote from './EvalQuote.svelte';
	import MarkdownText from '$lib/components/shared/MarkdownText.svelte';

	const TEXT =
		'Three rules applied, verified live on request `212850`: `firing_rule_ids [62, 63]`. ' +
		'A long line follows so the ten-line box has something to scroll. '.repeat(30) +
		'QUOTE-END';

	const { Story } = defineMeta({
		title: 'Eval/EvalQuote',
		component: EvalQuote,
		tags: ['autodocs']
	});
</script>

<Story
	name="Claude's last message in a scroll box"
	play={async ({ canvasElement }) => {
		const c = within(canvasElement);
		await expect(c.getByText("Claude's last message, 08:22")).toBeInTheDocument();
		const box = c.getByTestId('quoted');
		await expect(box.textContent).toContain('QUOTE-END');
		await expect(box.scrollHeight).toBeGreaterThan(box.clientHeight);
		const code = box.querySelector('code') as HTMLElement;
		await expect(getComputedStyle(code).backgroundColor).toMatch(/rgba\(0, 0, 0, 0\)|transparent/);
		await expect(canvasElement.querySelector('figure')?.getAttribute('data-who')).toBe('claude');
	}}
>
	{#snippet template()}
		<div style="max-width: 48rem; padding: var(--space-4); background: var(--surface-card);">
			<EvalQuote who="claude" heading="Claude's last message, 08:22" scroll testid="quoted">
				<MarkdownText text={TEXT} />
			</EvalQuote>
		</div>
	{/snippet}
</Story>
