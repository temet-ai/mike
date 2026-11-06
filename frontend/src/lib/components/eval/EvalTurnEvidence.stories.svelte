<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import EvalTurnEvidence from './EvalTurnEvidence.svelte';
	import {
		DETAIL_10487,
		FACTS_10487,
		QUOTE_10487,
		SESSION_10487
	} from '$lib/test-support/evalReviewFixtures';

	const HREF = `/sessions/${SESSION_10487}?request=212616`;

	const { Story } = defineMeta({
		title: 'Eval/EvalTurnEvidence',
		component: EvalTurnEvidence,
		tags: ['autodocs'],
		args: {
			context: DETAIL_10487.context,
			facts: { status: 'ok', facts: FACTS_10487 },
			keyAxes: ['delivery'],
			cited: { key: 'reply', span: QUOTE_10487 },
			turnNumber: 1,
			nextTurnNumber: 2,
			turnHref: HREF,
			onRetryFacts: fn()
		}
	});
</script>

<Story
	name="Turn 10487"
	play={async ({ canvasElement }) => {
		const c = within(canvasElement);
		await expect(canvasElement.textContent).toContain('This turn · turn 1');
		const heads = [...canvasElement.querySelectorAll('figcaption')].map((f) =>
			f.textContent?.replace(/\s+/g, ' ').trim()
		);
		await expect(heads[0]).toMatch(/^You wrote, \d\d:\d\d$/);
		await expect(heads[1]).toMatch(/^Claude's last message, \d\d:\d\d$/);
		await expect(heads[2]).toMatch(
			/^You replied, \d\d:\d\d this became turn 2 · marked: the part the labelling quoted$/
		);
		await expect(c.getByTestId('operator-prompt').textContent).toBe(
			DETAIL_10487.context?.user_prompt
		);
		const factsRow = c.getByTestId('turn-facts').textContent?.replace(/\s+/g, ' ') ?? '';
		await expect(factsRow).toMatch(
			/What it did ran 22 min 212 tool calls, 5 returned an error [▸ ]*34 files read or edited still running when you replied/
		);
		await expect(factsRow).not.toContain('ended normally');
		await expect(c.getByTestId('assistant-text').closest('.edge-blue')).not.toBeNull();
		await expect(c.getByTestId('next-message').querySelector('mark')?.textContent).toBe(
			QUOTE_10487
		);
		await expect(c.getByTestId('next-message').closest('.edge-blue')).toBeNull();
		await expect(c.getAllByRole('link')).toHaveLength(1);
		await expect(c.getByTestId('turn-link').getAttribute('href')).toBe(HREF);
		await expect(canvasElement.textContent).not.toMatch(/characters|Everything the assistant wrote|answer key/);

		await userEvent.click(c.getByRole('button', { name: /34 files read or edited/ }));
		await expect(canvasElement.querySelectorAll('li')).toHaveLength(34);
	}}
>
	{#snippet template(args)}
		<div class="edge mech-group" style="--edge: var(--edge-sm); max-width: 60rem;">
			<EvalTurnEvidence {...args} />
		</div>
	{/snippet}
</Story>
