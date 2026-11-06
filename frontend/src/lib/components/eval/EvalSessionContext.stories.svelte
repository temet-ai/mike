<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import EvalSessionContext from './EvalSessionContext.svelte';
	import {
		SESSION_24369,
		SUMMARY_24369,
		TURNS_24369
	} from '$lib/test-support/evalReviewFixtures';

	const { Story } = defineMeta({
		title: 'Eval/EvalSessionContext',
		component: EvalSessionContext,
		tags: ['autodocs'],
		args: {
			sessionUuid: SESSION_24369,
			summary: SUMMARY_24369,
			summaryError: null,
			turns: TURNS_24369,
			turnsError: null,
			position: { index: 9, total: 13 },
			open: true,
			onToggle: fn(),
			onRetry: fn()
		}
	});
</script>

<Story
	name="Session 24369cc6, turn 9 of 13"
	play={async ({ canvasElement, args }) => {
		const c = within(canvasElement);
		const band = c.getByTestId('session-band');
		await expect(band.textContent).toMatch(/13 turns · 27 Jul 2026 \d\d:\d\d to \d\d:\d\d/);
		await expect(c.getByRole('heading', { name: 'Review and refine the system prompt' })).toBeInTheDocument();
		await expect(c.getByTestId('open-session').getAttribute('href')).toBe(`/sessions/${SESSION_24369}`);
		await expect(c.getByTestId('session-overview').querySelectorAll('p')).toHaveLength(2);

		const before = c.getByTestId('just-before');
		await expect(before.textContent).toContain('Just before · turn 8');
		await expect(c.getByTestId('previous-prompt').textContent).toContain(
			'How do we make this easy to keep improving?'
		);
		const last = c.getByTestId('previous-last-message');
		await expect(last.textContent).toContain('Your worry is fair');
		await expect(last.getAttribute('role')).toBe('region');

		const earlier = c.getByRole('button', { name: 'Turns 1 to 7' });
		await userEvent.click(earlier);
		await expect(c.getByTestId('earlier-turns').children).toHaveLength(7);
		await userEvent.click(c.getByRole('button', { name: /The session/ }));
		await expect(args.onToggle).toHaveBeenCalledWith(false);
		await expect(canvasElement.textContent).not.toMatch(/\b(gold|answer key|seed|holdout)\b|\u2026/i);
	}}
>
	{#snippet template(args)}
		<div class="edge mech-group" style="--edge: var(--edge-sm); max-width: 60rem;">
			<EvalSessionContext {...args} />
		</div>
	{/snippet}
</Story>
