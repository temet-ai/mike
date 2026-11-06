<script module lang="ts">
	import { getContext } from 'svelte';
	import type { Component } from 'svelte';
	import { Bot, History, Activity, ScanSearch, Minimize2 } from '@lucide/svelte';
	import type { OperatorTurnDto } from '$lib/types';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';
	import { KIND_LABEL, KIND_TIP } from '$lib/components/shared/promptKind';
	import { parseTeammateMessages } from './teammateMessage';
	import { turnCollapseKey, highlightSegments } from './timelineTurnView';
	import { replyView } from './turnReply';

	const EVENT_PROMPT_KINDS = [
		'teammate-message',
		'session-recap',
		'background-event',
		'analysis-probe',
		'compaction'
	] as const;
	export type EventPromptKind = (typeof EVENT_PROMPT_KINDS)[number];

	export function isEventPromptKind(kind: string | null | undefined): kind is EventPromptKind {
		return kind != null && (EVENT_PROMPT_KINDS as readonly string[]).includes(kind);
	}

	const EVENT_KIND_ICON: Record<EventPromptKind, Component> = {
		'teammate-message': Bot,
		'session-recap': History,
		'background-event': Activity,
		'analysis-probe': ScanSearch,
		compaction: Minimize2
	};
</script>

<script lang="ts">
	interface Props {
		turn: OperatorTurnDto;
		requestRange?: { first: number | null; last: number | null };
		query: string;
	}

	let { turn, requestRange, query }: Props = $props();

	const collapseStore = getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY);
	const collapseKey = $derived(turnCollapseKey(turn));
	let localOpen = $state(false);
	const open = $derived(collapseStore ? !collapseStore.isCollapsed(collapseKey) : localOpen);

	function toggle(): void {
		if (collapseStore) collapseStore.toggle(collapseKey);
		else localOpen = !localOpen;
	}

	const kind = $derived(
		isEventPromptKind(turn.prompt_kind) ? turn.prompt_kind : 'background-event'
	);
	const Icon = $derived(EVENT_KIND_ICON[kind]);
	const label = $derived(KIND_LABEL[kind]);
	const tip = $derived(KIND_TIP[kind]);

	const reqRange = $derived.by(() => {
		const s = requestRange?.first ?? turn.start_request_number;
		const e = requestRange?.last ?? turn.end_request_number;
		return s === e ? `#${s}` : `#${s}–#${e}`;
	});
	const whenLabel = $derived(
		new Date(turn.created_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
	);
	const tokens = $derived.by(() => {
		const fmt = (n: number): string =>
			n >= 10000 ? `${Math.round(n / 1000)}k` : n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n);
		return `${fmt(turn.total_input_tokens)}↓ ${fmt(turn.total_output_tokens)}↑`;
	});

	const teammateMessages = $derived(
		kind === 'teammate-message' ? parseTeammateMessages(turn.user_prompt) : []
	);
	const senderLabel = $derived(
		teammateMessages.length > 0
			? [...new Set(teammateMessages.map((m) => m.teammateId))].join(', ')
			: null
	);
	const quotes = $derived(
		kind === 'teammate-message' && teammateMessages.length > 0
			? teammateMessages.map((m) => m.text)
			: [turn.user_prompt]
	);

	const reply = $derived(replyView(turn));
</script>

<div
	class="ser-root"
	data-slot="session-event-row"
	data-kind={turn.prompt_kind}
	data-band-id={turn.start_request_id}
>
	<button
		type="button"
		class="ser-toggle"
		aria-expanded={open}
		aria-label={`${open ? 'Collapse' : 'Expand'} ${turn.prompt_kind} event`}
		onclick={toggle}
	>
		<span class="ser-icon-box">
			<Icon size={12} aria-hidden="true" />
		</span>
		<span class="ser-label" title={tip}>{label}</span>
		<span class="ser-meta">
			{#if senderLabel}
				<span class="ser-sender">{senderLabel}</span>
				<span class="ser-sep" aria-hidden="true">→</span>
			{/if}
			<span>{reqRange}</span>
			<span class="ser-sep" aria-hidden="true">·</span>
			<span>{whenLabel}</span>
			<span class="ser-sep" aria-hidden="true">·</span>
			<span>{tokens}</span>
		</span>
	</button>
	{#if open}
		<div class="ser-body">
			{#each quotes as quoteText, i (i)}
				<p class="ser-quote">
					<span class="ser-quote-tag"
						>{quotes.length > 1 ? `Injected prompt ${i + 1}` : 'Injected prompt'}</span
					>
					<span class="ser-quote-text">
						{#each highlightSegments(quoteText, query) as seg, j (j)}
							{#if seg.mark}<mark class="ser-mark">{seg.text}</mark>{:else}{seg.text}{/if}
						{/each}
					</span>
				</p>
			{/each}
			{#if reply.text.trim().length > 0}
				<p class="ser-result">
					<span class="ser-result-tag">Result</span>
					<span class="ser-result-text">
						{#each highlightSegments(reply.text, query) as seg, j (j)}
							{#if seg.mark}<mark class="ser-mark">{seg.text}</mark>{:else}{seg.text}{/if}
						{/each}
					</span>
				</p>
			{/if}
		</div>
	{/if}
</div>

<style>
	.ser-root {
		display: flex;
		flex-direction: column;
		border-bottom: 1px solid var(--border-subtle);
	}
	.ser-toggle {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 6px 4px;
		border: none;
		background: transparent;
		text-align: left;
		font: inherit;
		color: inherit;
		cursor: pointer;
	}
	.ser-toggle:hover {
		background: var(--status-muted-bg);
	}
	.ser-toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}
	.ser-icon-box {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 20px;
		height: 20px;
		flex-shrink: 0;
		border-radius: 2px;
		background: var(--status-muted-bg);
		border: 1px solid var(--status-muted-border);
		color: var(--status-muted-fg);
	}
	.ser-label {
		flex-shrink: 0;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 700;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--status-muted-fg);
	}
	.ser-sender {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		font-weight: 600;
		color: var(--text-2);
	}
	.ser-meta {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		flex-shrink: 0;
		margin-left: auto;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-faint);
	}
	.ser-sep {
		color: var(--text-faint);
	}

	.ser-body {
		padding: 2px 4px 8px 32px;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.ser-quote,
	.ser-result {
		display: flex;
		align-items: baseline;
		flex-wrap: wrap;
		gap: 6px;
		margin: 0;
	}
	.ser-quote-tag,
	.ser-result-tag {
		flex-shrink: 0;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--text-faint);
	}
	.ser-quote-text {
		min-width: 0;
		flex: 1;
		font-size: 12px;
		font-style: italic;
		color: var(--text-3);
		white-space: pre-wrap;
		overflow-wrap: break-word;
		word-break: break-word;
	}
	.ser-result-text {
		min-width: 0;
		flex: 1;
		font-size: 12px;
		color: var(--text-2);
		white-space: pre-wrap;
		overflow-wrap: break-word;
		word-break: break-word;
	}
	:global(.ser-mark) {
		background: var(--interactive-active-bg);
		color: var(--interactive-active-fg);
	}
</style>
