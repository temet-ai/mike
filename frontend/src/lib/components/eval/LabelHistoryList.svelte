<script lang="ts">
	import type { EvalLabelDto } from '$lib/api/eval';
	import { Button } from '$lib/components/ui/button';
	import Badge from '$lib/components/shared/Badge.svelte';
	import { axisWord, canWithdraw, newestFirst, splitWord, tierWord, wordFor } from './labelControl';
	import { methodSentence } from './evalReview';

	interface Props {
		labels: EvalLabelDto[];
		busy: boolean;
		onWithdraw: (id: number) => void;
		onConfirm: (id: number) => void;
	}

	let { labels, busy, onWithdraw, onConfirm }: Props = $props();

	const rows = $derived(newestFirst(labels));

	function relayedQuote(label: EvalLabelDto): string | null {
		const quote = label.provenance['operator_quote'];
		return typeof quote === 'string' ? quote : null;
	}

	function relayedBy(label: EvalLabelDto): string {
		const by = label.provenance['relayed_by_session_uuid'];
		return typeof by === 'string' ? by : 'an unknown session';
	}
</script>

<ol class="lh-list" aria-label="Labels on this subject">
	{#each rows as row (row.id)}
		<li class="lh-row" data-label-id={row.id}>
			<div class="lh-head">
				<Badge variant={row.is_current ? 'chip-active' : 'chip-muted'}>{tierWord(row.tier)}</Badge>
				<span class="lh-axis">{axisWord(row.axis)}</span>
				<span class="lh-label"
					>{row.action === 'withdraw'
						? `withdrawn ${wordFor(row.label)}`
						: wordFor(row.label)}</span
				>
				{#if splitWord(row.split)}<Badge variant="chip-muted">{splitWord(row.split)}</Badge>{/if}
				<span class="lh-meta"
					>{methodSentence(row.method)} · {new Date(row.created_at).toLocaleString()}</span
				>
			</div>
			{#if row.evidence}<p class="lh-text">{row.evidence}</p>{/if}
			{#if row.note}<p class="lh-text">{row.note}</p>{/if}
			{#if row.tier === 'relayed' && relayedQuote(row)}
				<p class="lh-text">
					Relayed by session <code>{relayedBy(row)}</code> quoting: “{relayedQuote(row)}”
					<span class="lh-meta"
						>{row.provenance['relayed_by_verified'] === true
							? 'quote verified'
							: 'not verified'}</span
					>
				</p>
			{/if}
			{#if row.tier === 'seed' && row.method === 'double_blind_adjudicated'}
				<p class="lh-meta">
					double-blind, {row.provenance['adjudicated'] === true ? 'adjudicated' : 'agreed'}
				</p>
			{/if}
			{#if canWithdraw(row) || (row.tier === 'relayed' && row.is_current)}
				<div class="lh-actions">
					{#if canWithdraw(row)}
						<Button variant="outline" size="sm" disabled={busy} onclick={() => onWithdraw(row.id)}
							>Withdraw</Button
						>
					{/if}
					{#if row.tier === 'relayed' && row.is_current}
						<Button size="sm" disabled={busy} onclick={() => onConfirm(row.id)}
							>Confirm label</Button
						>
					{/if}
				</div>
			{/if}
		</li>
	{/each}
</ol>

<style>
	.lh-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.lh-row {
		border-top: 1px solid var(--border-subtle);
		padding-top: var(--space-2);
	}
	.lh-head {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		align-items: center;
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.lh-axis {
		color: var(--text-3);
	}
	.lh-label {
		color: var(--text-strong);
	}
	.lh-text {
		margin: var(--space-1) 0 0;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		font-size: var(--text-xs);
		color: var(--text-1);
	}
	.lh-meta {
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.lh-actions {
		display: flex;
		gap: var(--space-2);
		margin-top: var(--space-2);
	}
</style>
