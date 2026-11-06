<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import EvalAnswerCard from './EvalAnswerCard.svelte';
	import { labellingWho, labellingWhy } from './evalReview';
	import {
		DETAIL_10695,
		FACT_VOCABULARY,
		HISTORY_10695,
		VOCABULARY
	} from '$lib/test-support/evalReviewFixtures';

	const ROW = DETAIL_10695.labels.current[0]!;
	const CORRECTED =
		VOCABULARY.find((a) => a.axis === 'reaction')?.labels.find((l) => l.label === 'corrected')
			?.description ?? null;
	const NOW = new Date(2026, 8, 28, 12, 0);

	const { Story } = defineMeta({
		title: 'Eval/EvalAnswerCard',
		component: EvalAnswerCard,
		tags: ['autodocs'],
		args: {
			axis: 'reaction',
			label: 'corrected',
			holders: ['label'],
			definition: CORRECTED,
			labelledBy: labellingWho(ROW, 'reaction', NOW),
			why: labellingWhy({
				labels: HISTORY_10695,
				row: ROW,
				axis: 'reaction',
				vocabulary: [...VOCABULARY, ...FACT_VOCABULARY],
				quoteShown: true,
				now: NOW
			}),
			verdict: DETAIL_10695.item.verdict,
			answeredBy: null,
			judgeSaw: null,
			notGraded: null
		}
	});
</script>

<Story
	name="This turn's label, turn 10695"
	play={async ({ canvasElement }) => {
		const c = within(canvasElement);
		await expect(c.getByRole('heading', { level: 3 }).textContent?.trim()).toBe(
			"This turn's label"
		);
		await expect(canvasElement.textContent).toContain('Corrected');
		await expect(c.getByTestId('labelled-by').textContent).toBe(
			'Two AI models (Sonnet and Opus), working separately, on 28 Sep. Both said Corrected.'
		);
		await expect(c.queryByText('Its reason')).toBeNull();
		const toggle = c.getByRole('button', { name: 'Why it is labelled Corrected' });
		await expect(toggle).toHaveAttribute('aria-expanded', 'false');
		await userEvent.click(toggle);
		const why = c.getByTestId('why-labelled');
		await expect(why.textContent).toContain('Sonnet');
		await expect(why.textContent).toContain('On 25 Sep, one AI model also said Corrected');
		const rules = within(why).getByTestId('labelling-rules');
		await expect(rules.hasAttribute('open')).toBe(false);
		await expect(rules.textContent).toContain('How mixed cases are labelled');
		const noted = [...why.querySelectorAll('li')].filter((li) => !rules.contains(li));
		await expect(noted.map((li) => li.textContent)).toEqual([
			"This turn's own prompt corrects an earlier turn.",
			'Stopped short.',
			'Hands back something the assistant could do itself: asks permission for, or flags as a decision, something already authorised or within its remit.',
			'Asked instead of acting.'
		]);
		await expect(canvasElement.textContent).not.toMatch(
			/\b(gold|answer key|seed|holdout|method|tier|jc11)\b|\d\d:\d\d/i
		);
	}}
/>
