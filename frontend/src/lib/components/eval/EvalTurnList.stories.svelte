<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
	import EvalTurnList from './EvalTurnList.svelte';
	import type { RowInfo } from './EvalTurnList.svelte';
	import { RUN3_DELIVERY_FIRST_10 } from '$lib/test-support/evalReviewFixtures';

	const ITEMS = RUN3_DELIVERY_FIRST_10.map((r, i) =>
		r.operator_turn_id === 10487
			? { ...r, reviewed_at: '2026-09-28T09:40:00Z' }
			: i === 3
				? {
						...r,
						session_uuid: '0b7e1f7a-0000-4000-8000-000000000003',
						session_title: 'Title from the list'
					}
				: r
	);
	const INFO = new Map<number, RowInfo>([
		[10487, { title: 'Build a sign-in timing extension', position: { index: 1, total: 8 } }],
		[10954, { title: 'Review the prompt stack', position: { index: 4, total: 13 } }]
	]);

	const { Story } = defineMeta({
		title: 'Eval/EvalTurnList',
		component: EvalTurnList,
		tags: ['autodocs'],
		args: {
			items: ITEMS,
			mode: 'delivery',
			selected: 10954,
			info: INFO,
			onVisible: fn(),
			onSelect: fn()
		}
	});
</script>

<Story
	name="Run 3 delivery turns"
	play={async ({ canvasElement, args }) => {
		const c = within(canvasElement);
		const rows = c.getAllByRole('button');
		await expect(rows.map((r) => r.getAttribute('data-turn'))).toEqual(
			ITEMS.map((i) => String(i.operator_turn_id))
		);
		await expect(rows[0]?.textContent).toMatch(
			/Build a sign-in timing extension\s*Partial\s*Turn 1 of 8 · the label says Partial · the judge says Delivered/
		);
		await expect(rows[1]).toHaveAttribute('aria-current', 'true');
		await expect(rows[2]?.textContent).toContain('Loading session');
		await expect(rows[3]?.textContent).toContain('Title from the list');
		await waitFor(() => expect(args.onVisible).toHaveBeenCalledWith(10487));
		const both = canvasElement.querySelector('[data-turn="12180"]') as HTMLElement;
		await expect(both.textContent).toContain(
			'How you reacted: the label says Corrected · the judge says Accepted'
		);
		const left = both.getBoundingClientRect().left;
		await userEvent.click(both);
		await expect(args.onSelect).toHaveBeenCalledWith(12180);
		await expect(both.getBoundingClientRect().left).toBe(left);
	}}
/>
