<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import SeatDecisions from './SeatDecisions.svelte';
	import type { SeatDecisionFigures } from './seatDecisions';

	const STOPS = Array.from({ length: 12 }, (_, i) => ({
		id: 100 - i,
		at: `2026-11-14T10:${String(59 - i).padStart(2, '0')}:00Z`,
		result: i === 0 ? 'pushed back' : i === 3 ? 'fell back' : 'let through',
		score: i === 3 ? null : 0.2 + i / 40
	}));

	const FIGURES: SeatDecisionFigures = {
		effort: { work: 620, machinery: 210, waiting: 170 },
		mail: { held: 7, woken: 12 },
		stopChecks: STOPS,
		triage: [
			{
				id: 9,
				at: '2026-11-14T10:30:00Z',
				exitCode: 1,
				line: 'triage: fix-code. The failing test names a function this seat changed, so the fix belongs in its own files rather than in a retry of the check.'
			}
		],
		commitMessages: [
			{ id: 4, at: '2026-11-14T10:20:00Z', result: 'warned', score: 0.31 },
			{ id: 3, at: '2026-11-14T10:10:00Z', result: 'no warning', score: 0.82 }
		]
	};

	const EMPTY: SeatDecisionFigures = {
		effort: { work: 0, machinery: 0, waiting: 0 },
		mail: { held: 0, woken: 0 },
		stopChecks: [],
		triage: [],
		commitMessages: []
	};

	const { Story } = defineMeta({
		title: 'Fleet/SeatDecisions',
		component: SeatDecisions,
		tags: ['autodocs'],
		argTypes: { seat: { control: false }, modes: { control: false } }
	});
</script>

<Story
	name="Default"
	tags={['interactive']}
	args={{
		seat: FIGURES,
		modes: { effort: 'on', wake_check: 'off', stop_check: 'shadow' }
	}}
	play={async ({ canvasElement }) => {
		const blocks = Array.from(
			canvasElement.querySelectorAll<HTMLElement>('[data-slot="seat-decisions"]')
		);
		await expect(blocks).toHaveLength(2);
		const [live, empty] = blocks as [HTMLElement, HTMLElement];
		const block = (root: HTMLElement, name: string) =>
			root.querySelector<HTMLElement>(`[data-block="${name}"]`) as HTMLElement;

		await expect(live.textContent?.replace(/\s+/g, ' ')).toContain(
			'work 62% · machinery 21% · waiting 17%'
		);
		await expect(block(live, 'mail').textContent).toContain('off');
		await expect(block(live, 'mail').textContent).toContain('12');
		await expect(block(live, 'effort').textContent).not.toContain('off');
		await expect(block(live, 'stop-checks').textContent).toContain('shadow');
		await expect(block(live, 'stop-checks').textContent).toContain('Stop checks 12');
		await expect(block(live, 'stop-checks').textContent).toContain('pushed back');
		await expect(block(live, 'stop-checks').textContent).toContain('fell back');
		await expect(block(live, 'triage').textContent).toContain('fix-code');
		await expect(block(live, 'triage').textContent).toContain('exit 1');
		await expect(block(live, 'commit-messages').textContent).toContain('warned');

		const stopList = block(live, 'stop-checks').querySelector<HTMLElement>(
			'.sd-list'
		) as HTMLElement;
		await expect(stopList.scrollHeight).toBeGreaterThan(stopList.clientHeight);
		await expect(stopList.clientHeight).toBeLessThanOrEqual(160);
		await expect(stopList.querySelectorAll('[role="listitem"]')).toHaveLength(12);

		for (const name of ['effort', 'stop-checks', 'triage', 'commit-messages']) {
			await expect(block(empty, name).textContent).toContain('—');
		}
		await expect(block(empty, 'stop-checks').textContent).toContain('Stop checks 0');
		await expect(canvasElement.textContent).toContain('Figures arrive when the run closes.');

		const emptyList = block(empty, 'triage').querySelector<HTMLElement>('.sd-list') as HTMLElement;
		await expect(Math.round(emptyList.getBoundingClientRect().height)).toBe(20);
	}}
>
	{#snippet template(args)}
		<div
			style="width: 22rem; display: flex; flex-direction: column; gap: var(--space-4); background: var(--surface-2); padding: var(--space-3);"
		>
			<SeatDecisions {...args} />
			<SeatDecisions seat={EMPTY} modes={null} />
			<SeatDecisions seat={null} unavailableReason="Figures arrive when the run closes." />
		</div>
	{/snippet}
</Story>
