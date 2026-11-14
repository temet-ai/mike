<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import type { Precheck } from '$lib/api/steering';
	import PrecheckAnswers from './PrecheckAnswers.svelte';

	const HELD: Precheck = {
		record_id: 12,
		question_set: 'turn_guard_precheck_v2',
		answers: { already_disclosed: 0.91, claims_checked: 0.04, invites_trial: 0.88 },
		refusals: 1,
		verdict: {
			send: false,
			score: 0.05,
			why: 'the message says what was not checked and asks the operator to try it'
		},
		fail_open: false,
		error: null
	};

	const FAILED_OPEN: Precheck = {
		record_id: null,
		question_set: 'turn_guard_precheck_v2',
		answers: {},
		refusals: 0,
		verdict: { send: true, score: 1, why: 'the pre-check failed, so the fire is sent: timeout' },
		fail_open: true,
		error: 'the decision call timed out after 3000 ms'
	};

	const { Story } = defineMeta({
		title: 'Components/Shell/PrecheckAnswers',
		component: PrecheckAnswers,
		tags: ['autodocs'],
		argTypes: { precheck: { control: false } }
	});
</script>

<Story
	name="Default"
	tags={['interactive']}
	args={{ precheck: HELD, applies: true }}
	play={async ({ canvasElement }) => {
		const blocks = Array.from(
			canvasElement.querySelectorAll<HTMLElement>('[data-slot="precheck-answers"]')
		);
		await expect(blocks).toHaveLength(3);
		const [held, failed, notRun] = blocks;

		await expect(held?.textContent?.replace(/\s+/g, ' ')).toContain('HELD 0.05');
		await expect(held?.querySelectorAll('[data-answer]')).toHaveLength(4);
		await expect(
			within(held as HTMLElement)
				.getByText('Is an answer, not a change')
				.closest('[data-answer]')?.textContent
		).toContain('—');
		await expect(held?.textContent).toContain('1 refused');

		await expect(failed?.textContent?.replace(/\s+/g, ' ')).toContain('SENT 1.00');
		await expect(failed?.textContent).toContain('Failed open');
		await expect(failed?.textContent).toContain('timed out after 3000 ms');
		await expect(failed?.querySelectorAll('[data-answer]')).toHaveLength(4);

		await expect(notRun?.textContent).toContain('Pre-check not run');
		await expect(notRun?.querySelectorAll('[data-answer]')).toHaveLength(0);

		const rowHeights = Array.from(held?.querySelectorAll<HTMLElement>('[data-answer]') ?? []).map(
			(row) => Math.round(row.getBoundingClientRect().height)
		);
		await expect(new Set(rowHeights)).toEqual(new Set([24]));
	}}
>
	{#snippet template(args)}
		<div
			style="width: 28rem; display: flex; flex-direction: column; gap: var(--space-4); background: var(--surface-card); padding: var(--space-3);"
		>
			<PrecheckAnswers {...args} />
			<PrecheckAnswers precheck={FAILED_OPEN} applies={true} />
			<PrecheckAnswers precheck={null} applies={true} />
			<PrecheckAnswers precheck={HELD} applies={false} />
		</div>
	{/snippet}
</Story>
