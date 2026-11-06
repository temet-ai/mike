<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import LabelHistoryList from './LabelHistoryList.svelte';
	import type { EvalLabelDto } from '$lib/api/eval';

	const EVIDENCE = 'just to be clear. you have now wasted hours on this quick test. '.repeat(8);

	function row(o: Partial<EvalLabelDto>): EvalLabelDto {
		return {
			id: 1,
			subject_kind: 'turn',
			subject_id: 16368,
			operator_turn_id: 16368,
			session_uuid: 's',
			axis: 'reaction',
			label: 'corrected',
			action: 'assert',
			tier: 'seed',
			method: 'double_blind_adjudicated',
			split: 'dev',
			tags: [],
			severity: 'major',
			confidence: 'high',
			evidence: EVIDENCE,
			note: null,
			source: 'fleet_mirror/ground_truth',
			provenance: { adjudicated: true },
			verdict_snapshot: null,
			supersedes_id: null,
			created_at: '2026-09-24T10:00:00Z',
			is_current: true,
			...o
		};
	}

	const ROWS: EvalLabelDto[] = [
		row({ id: 1 }),
		row({
			id: 2,
			tier: 'relayed',
			method: 'relayed_quote',
			source: 'mcp_relay',
			evidence: null,
			provenance: {
				operator_quote: 'that turn was fine, it was done',
				relayed_by_session_uuid: 'abc',
				relayed_by_verified: true
			}
		})
	];

	const { Story } = defineMeta({
		title: 'Eval/LabelHistoryList',
		component: LabelHistoryList,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	args={{ labels: ROWS, busy: false, onWithdraw: () => {}, onConfirm: () => {} }}
	play={async ({ canvasElement }) => {
		await expect(canvasElement.textContent).toContain(EVIDENCE.trim());
		await expect(canvasElement.textContent).toContain('quote verified');
		await expect(canvasElement.textContent).toContain('double-blind, adjudicated');
		await expect(canvasElement.textContent).toContain('Confirm label');
	}}
/>
