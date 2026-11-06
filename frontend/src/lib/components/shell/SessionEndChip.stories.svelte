<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import SessionEndChip from './SessionEndChip.svelte';

	const { Story } = defineMeta({
		title: 'Shell/SessionEndChip',
		component: SessionEndChip,
		tags: ['autodocs']
	});

	const ENDS = [
		{
			end_label: 'closed_in_frustration',
			end_source: 'llm',
			end_rule: null,
			end_confidence: 'high',
			end_evidence: "operator: forget it, I'll do it myself",
			failure_tags: ['unverified_claim']
		},
		{
			end_label: 'fleet_seat',
			end_source: 'deterministic',
			end_rule: 'fleet-seat-v1',
			end_confidence: 'high',
			end_evidence: 'seat w3 of run r1',
			failure_tags: []
		},
		{ end_label: null }
	];
</script>

<Story
	name="SessionEndChip"
	play={async ({ canvasElement }) => {
		const labels = [...canvasElement.querySelectorAll('[data-session-end]')].map((el) =>
			el.getAttribute('data-session-end')
		);
		await expect(labels).toEqual(['closed_in_frustration', 'fleet_seat']);
		const seat = canvasElement.querySelector('[data-session-end="fleet_seat"]');
		await expect(seat?.querySelector('[data-end-axis="origin"]')?.textContent?.trim()).toBe(
			'rule fleet-seat-v1'
		);
		await expect(canvasElement.textContent).toContain('unverified claim');
		const link = canvasElement.querySelector('a[href="/api/sessions/s-1/verdicts"]');
		await expect(link).not.toBeNull();
	}}
>
	{#snippet template()}
		<div class="flex flex-col gap-2 bg-[var(--surface-bg)] p-4">
			{#each ENDS as end, i (i)}
				<SessionEndChip {end} sessionId="s-{i + 1}" />
			{/each}
		</div>
	{/snippet}
</Story>
