<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import EvalDecisionBar from './EvalDecisionBar.svelte';
	import type { BarQuestion } from './EvalDecisionBar.svelte';
	import { candidates } from './evalReview';
	import { optionsFor } from './labelControl';
	import { DETAIL_12180, TURN_12180, VOCABULARY } from '$lib/test-support/evalReviewFixtures';

	const QUESTIONS: BarQuestion[] = (['delivery', 'reaction'] as const).map((axis) => {
		const list = candidates(TURN_12180, DETAIL_12180.labels.current, axis);
		return {
			axis,
			candidates: list,
			others: optionsFor(VOCABULARY, axis).filter((o) => !list.some((c) => c.label === o.label)),
			saved: null
		};
	});

	const { Story } = defineMeta({
		title: 'Eval/EvalDecisionBar',
		component: EvalDecisionBar,
		tags: ['autodocs'],
		args: {
			questions: QUESTIONS,
			savedAt: null,
			busy: false,
			status: null,
			note: '',
			onDecide: fn(),
			onStatus: fn(),
			onGoBack: fn()
		}
	});
</script>

<Story
	name="Two questions, turn 12180"
	args={{ suggestion: { label: 'accepted', probability: 0.72 } }}
	play={async ({ canvasElement, args }) => {
		const c = within(canvasElement);
		await expect(c.getByText('Which is right?')).toBeInTheDocument();
		await expect(c.getByTestId('prelabel-suggestion').textContent).toBe(
			'Jev suggests Accepted, 72%'
		);
		await expect(
			canvasElement.querySelectorAll('button[data-label][aria-pressed="true"]')
		).toHaveLength(0);
		const rows = c.getAllByRole('group');
		await expect(rows.map((r) => r.getAttribute('aria-label'))).toEqual([
			'What was delivered',
			'How you reacted'
		]);
		const delivery = within(rows[0] as HTMLElement);
		const reaction = within(rows[1] as HTMLElement);
		await expect(
			delivery.getByRole('button', { name: /^Partial\s*the label$/i })
		).toBeInTheDocument();
		await expect(
			delivery.getByRole('button', { name: /^Delivered\s*the judge$/i })
		).toBeInTheDocument();
		await expect(
			[...(rows[0] as HTMLElement).querySelectorAll('button[data-label]')].map((b) =>
				b.getAttribute('data-label')
			)
		).toEqual(['partial', 'delivered', 'shipped', 'none', 'question', 'not_applicable']);
		await expect(canvasElement.textContent).toContain('Something else:');
		await expect(canvasElement.querySelector('[aria-haspopup="dialog"][data-label]')).toBeNull();
		await expect(canvasElement.textContent).not.toMatch(/Neither|Alt\+|answer key/);

		const answers = [...canvasElement.querySelectorAll<HTMLElement>('button[data-label]')];
		const heights = new Set(answers.map((b) => Math.round(b.getBoundingClientRect().height)));
		await expect(heights.size).toBe(1);
		const before = answers.map((b) => b.getBoundingClientRect().left);

		await expect(c.getByTestId('decision-status').textContent?.trim()).toBe(
			'Two questions: pick an answer in each row. The turn saves once both have one.'
		);
		const partial = delivery.getByRole('button', { name: /^Partial/i });
		await userEvent.click(partial);
		await expect(partial).toHaveAttribute('aria-pressed', 'true');
		await expect(c.getByTestId('decision-status').textContent?.trim()).toBe(
			'Now answer the other row; the turn saves once both have an answer.'
		);
		await expect(args.onDecide).not.toHaveBeenCalled();
		await expect(answers.map((b) => b.getBoundingClientRect().left)).toEqual(before);

		await userEvent.click(reaction.getByRole('button', { name: /^Redirected$/i }));
		await expect(args.onDecide).toHaveBeenCalledWith([
			{ axis: 'delivery', label: 'partial' },
			{ axis: 'reaction', label: 'redirected' }
		]);
	}}
/>
