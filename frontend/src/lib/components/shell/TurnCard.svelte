<script lang="ts">
	import { getContext, untrack } from 'svelte';
	import {
		ChevronDown,
		MessageSquare,
		Bot,
		CornerDownRight,
		Brain,
		Wrench,
		Workflow
	} from '@lucide/svelte';
	import type { OperatorTurnDto, ToolUseDto } from '$lib/types';
	import { cn } from '$lib/utils';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';
	import { getResponseBody } from '$lib/api';
	import { walkResponseContent } from '$lib/components/message-view/messageWalker';
	import { replyView } from './turnReply';
	import ToolCallCard from '$lib/components/tool-calls/ToolCallCard.svelte';
	import WorkflowRunBand from './WorkflowRunBand.svelte';
	import GoldLabelControl from '$lib/components/eval/GoldLabelControl.svelte';
	import { Button } from '$lib/components/ui/button';
	import Badge from '$lib/components/shared/Badge.svelte';
	import MarkdownText from '$lib/components/shared/MarkdownText.svelte';
	import PromptKindBadge from '$lib/components/shared/PromptKindBadge.svelte';
	import TurnVerdictChips from './TurnVerdictChips.svelte';
	import { parseTeammateMessages, teammateMessagePreview } from './teammateMessage';
	import { disclose } from '$lib/components/ui/motion';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import {
		categoryLabel,
		categoryColorVar,
		outcomeColorVar,
		outcomeLabel,
		formatTokenCount,
		filesTouchedOf,
		computeTurnToolStats,
		toolCallMatchesQuery,
		splitFirstLine,
		highlightSegments,
		turnCollapseKey,
		ledgerCellState,
		type ToolCategory
	} from './timelineTurnView';

	interface Props {
		turn: OperatorTurnDto;
		index: number;
		isFirst: boolean;
		toolCalls: ToolUseDto[];
		ledgerUnavailable: boolean;
		live: boolean;
		now: number;
		defaultExpanded: boolean;
		query: string;
		toolFilter: string;
		forcedOpen: boolean | null;
		flash: boolean;
		jumpTargetRequestId: number | null;
		requestNumberFor: (id: number) => number | null;
		requestRange?: { first: number | null; last: number | null };
		statusFor: (id: number) => string;
		onManualToggle: () => void;
		onOpenDrawer: (id: number, kind: 'request' | 'response') => void;
		onJumpSelf: () => void;
	}

	let {
		turn,
		index,
		isFirst,
		toolCalls,
		ledgerUnavailable,
		live,
		now,
		defaultExpanded,
		query,
		toolFilter,
		forcedOpen,
		flash,
		jumpTargetRequestId,
		requestNumberFor,
		requestRange,
		statusFor,
		onManualToggle,
		onOpenDrawer,
		onJumpSelf
	}: Props = $props();

	const collapseStore = getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY);
	const collapseKey = $derived(turnCollapseKey(turn));
	let localOpen = $state(untrack(() => defaultExpanded));

	const ownOpen = $derived(collapseStore ? !collapseStore.isCollapsed(collapseKey) : localOpen);
	const open = $derived(forcedOpen ?? ownOpen);

	function toggle(): void {
		onManualToggle();
		if (collapseStore) {
			collapseStore.toggle(collapseKey);
		} else {
			localOpen = !localOpen;
		}
	}

	const reqRange = $derived.by(() => {
		const s = requestRange?.first ?? turn.start_request_number;
		const e = requestRange?.last ?? turn.end_request_number;
		return s === e ? `#${s}` : `#${s}–#${e}`;
	});

	const outcome = $derived(turn.outcome);
	const accentVar = $derived(outcomeColorVar(outcome, live));
	const totalTokens = $derived(
		formatTokenCount(turn.total_input_tokens) +
			'↓ ' +
			formatTokenCount(turn.total_output_tokens) +
			'↑'
	);
	const whenLabel = $derived(
		new Date(turn.created_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
	);

	const displayPrompt = $derived(
		turn.user_prompt
			.replace(/<system-reminder>[\s\S]*?<\/system-reminder>/g, '')
			.replace(
				/<[a-z][a-z0-9-]*(?::[a-z][a-z0-9-]*)?>[\s\S]*?<\/[a-z][a-z0-9-]*(?::[a-z][a-z0-9-]*)?>|<[a-z][a-z0-9-]*(?::[a-z][a-z0-9-]*)?\s*\/>/g,
				''
			)
			.trim()
	);
	const promptPreview = $derived(splitFirstLine(displayPrompt));

	const teammateMessages = $derived(
		turn.prompt_kind === 'teammate-message' ? parseTeammateMessages(turn.user_prompt) : []
	);
	const isTeammateMessage = $derived(teammateMessages.length > 0);
	const headerPreviewText = $derived(
		isTeammateMessage ? teammateMessagePreview(teammateMessages) : promptPreview.first
	);

	const reply = $derived(replyView(turn));
	const replyPreview = $derived(splitFirstLine(reply.text));

	const stats = $derived(computeTurnToolStats(toolCalls));
	const ledgerState = $derived(
		ledgerCellState(ledgerUnavailable, toolCalls.length, turn.tool_call_count)
	);
	const catChips = $derived.by(() => {
		const cats: ToolCategory[] = ['read', 'edit', 'exec', 'dispatch'];
		return cats
			.filter((c) => stats.catCounts[c] > 0)
			.map((c) => ({
				cat: c,
				label: categoryLabel(c),
				count: stats.catCounts[c],
				colorVar: categoryColorVar(c)
			}));
	});
	const fileCount = $derived(filesTouchedOf(turn).length);
	const toolSummary = $derived(
		`${turn.tool_call_count} tool call${turn.tool_call_count === 1 ? '' : 's'}` +
			(fileCount > 0 ? ` · ${fileCount} file${fileCount === 1 ? '' : 's'} changed` : '')
	);

	let showAllTools = $state(false);
	const SHOWN_CAP = 24;
	const jumpTargetInThisTurn = $derived(
		jumpTargetRequestId !== null && toolCalls.some((t) => t.request_id === jumpTargetRequestId)
	);
	$effect(() => {
		if (jumpTargetInThisTurn) showAllTools = true;
	});
	const visibleCalls = $derived(showAllTools ? toolCalls : toolCalls.slice(0, SHOWN_CAP));
	const hasMoreTools = $derived(toolCalls.length > visibleCalls.length);
	const maxBytes = $derived(Math.max(1, ...toolCalls.map((t) => t.result?.content_bytes ?? 0)));

	const workflowRequestIds = $derived.by(() => {
		const seen = new Set<number>();
		for (const t of toolCalls) {
			if (t.tool_name === 'Workflow') seen.add(t.request_id);
		}
		return Array.from(seen);
	});

	type ReasoningState =
		| { kind: 'idle' }
		| { kind: 'loading' }
		| { kind: 'ready'; text: string }
		| { kind: 'none' }
		| { kind: 'error' };
	let reasoningState = $state<ReasoningState>({ kind: 'idle' });
	let reasoningOpen = $state(false);

	$effect(() => {
		if (!open || live) return;
		untrack(() => {
			if (reasoningState.kind !== 'idle') return;
			reasoningState = { kind: 'loading' };
			void getResponseBody(turn.end_request_id)
				.then((payload) => {
					if (!payload.text) {
						reasoningState = { kind: 'none' };
						return;
					}
					let parsed: unknown;
					try {
						parsed = JSON.parse(payload.text);
					} catch {
						reasoningState = { kind: 'none' };
						return;
					}
					const msg = walkResponseContent(parsed, 0);
					const thinking = (msg?.blocks ?? [])
						.filter((b): b is { kind: 'thinking'; text: string } => b.kind === 'thinking')
						.map((b) => b.text)
						.join('\n\n');
					reasoningState =
						thinking.trim().length > 0 ? { kind: 'ready', text: thinking } : { kind: 'none' };
				})
				.catch(() => {
					reasoningState = { kind: 'error' };
				});
		});
	});
</script>

<div class="tc-elev" data-elevation="card" data-elevation-raise data-motion="hover">
	<article
		class={cn('tc', flash && 'tc--flash')}
		style="--tc-accent: var({accentVar});"
		data-slot="turn-card"
		data-band-id={turn.start_request_id}
		data-turn-id={turn.turn_id}
		data-outcome={outcome}
		data-augmented-ui="tl-clip br-clip border"
	>
		<header class="tc-header">
			<div class="flex items-center gap-2.5 px-3 py-2">
				<button
					type="button"
					class="group flex min-w-0 flex-1 items-center gap-2.5 text-left"
					data-elevation-trigger
					data-motion="hover"
					aria-expanded={open}
					aria-label={`${open ? 'Collapse' : 'Expand'} operator turn ${index}`}
					onclick={toggle}
				>
					<DisclosureMarker {open} size={14} class="shrink-0" />
					<span
						class="font-display shrink-0 rounded-[2px] px-1.5 py-0.5 text-xs font-bold"
						style="color: var(--tc-accent); background: color-mix(in srgb, var(--tc-accent) 12%, transparent); border: 1px solid color-mix(in srgb, var(--tc-accent) 30%, transparent);"
						>{String(index).padStart(2, '0')}</span
					>
					{#if isTeammateMessage}
						<Bot size={12} class="shrink-0 text-[var(--data-teal-fg)]" aria-hidden="true" />
					{:else}
						<MessageSquare
							size={12}
							class="shrink-0 text-[var(--data-teal-fg)]"
							aria-hidden="true"
						/>
					{/if}
					<span class="tc-prompt-preview min-w-0 flex-1 text-xs text-[var(--text-1)]">
						{#if headerPreviewText.trim().length === 0}
							<span class="font-mono text-xs italic text-[var(--text-faint)]"
								>continuation — no operator message</span
							>
						{:else}
							{#each highlightSegments(headerPreviewText, query) as seg, i (i)}
								{#if seg.mark}<mark
										class="bg-[var(--interactive-active-bg)] text-[var(--interactive-active-fg)]"
										>{seg.text}</mark
									>{:else}{seg.text}{/if}
							{/each}
						{/if}
					</span>
				</button>
				<Button
					variant="outline"
					size="sm"
					class="shrink-0"
					onclick={() => onOpenDrawer(turn.start_request_id, 'request')}>view request</Button
				>
				<span class="flex shrink-0 items-center gap-2.5">
					<span class="font-mono text-micro tabular-nums text-[var(--text-faint)]">{whenLabel}</span
					>
					<span class="font-mono text-micro tabular-nums text-[var(--text-3)]">{totalTokens}</span>
					<span
						class="inline-flex items-center gap-1.5 text-micro font-semibold uppercase tracking-wider"
						style="color: var(--tc-accent);"
					>
						{outcomeLabel(outcome, live)}
					</span>
					<TurnVerdictChips {turn} />
				</span>
			</div>
			<div class="flex flex-wrap items-center gap-2 px-3 pb-2 pl-9">
				<button
					type="button"
					class="rounded-[2px] border border-[var(--border-subtle)] bg-[var(--surface-bg)] px-1.5 py-0.5 font-mono text-micro tabular-nums text-[var(--text-3)] hover:border-[var(--interactive-active-border)] hover:text-[var(--interactive-active-fg)]"
					title={`Requests ${reqRange}`}
					onclick={onJumpSelf}>{reqRange}</button
				>
				<PromptKindBadge kind={turn.prompt_kind} />
				{#each catChips as chip (chip.cat)}
					<span
						class="inline-flex items-center gap-1 font-mono text-micro tabular-nums text-[var(--text-3)]"
					>
						<span
							class="size-[7px] rounded-[2px]"
							style="background: var({chip.colorVar});"
							aria-hidden="true"
						></span>
						{chip.count}
						{chip.label}
					</span>
				{/each}
				{#if stats.errorCount > 0}
					<span
						class="inline-flex items-center gap-1 font-mono text-micro tabular-nums text-[var(--status-danger-fg)]"
					>
						{stats.errorCount} err
					</span>
				{/if}
			</div>
		</header>

		{#if open}
			<div class="tc-body" transition:disclose>
				<div class="tc-body__inner">
					{#if reasoningState.kind === 'ready'}
						<button
							type="button"
							class="flex w-full items-center gap-2 border-b border-[var(--border-subtle)] bg-[var(--surface-bg)] px-3.5 py-1.5 pl-9 text-left text-[var(--text-3)] hover:text-[var(--text-strong)]"
							aria-expanded={reasoningOpen}
							onclick={() => (reasoningOpen = !reasoningOpen)}
						>
							<DisclosureMarker open={reasoningOpen} size={11} />
							<Brain size={11} class="text-[var(--status-steering-fg)]" aria-hidden="true" />
							<span class="text-label tracking-wide">Reasoning</span>
							<span class="text-micro text-[var(--text-faint)]"
								>{reasoningState.text.length} chars</span
							>
						</button>
						{#if reasoningOpen}
							<div
								class="border-b border-[var(--border-subtle)] bg-[var(--surface-bg)] px-3.5 py-2.5 pl-[52px]"
							>
								<p
									class="m-0 whitespace-pre-wrap text-label italic leading-relaxed text-[var(--text-3)]"
								>
									{reasoningState.text}
								</p>
							</div>
						{/if}
					{/if}

					<div
						class="border-b border-[var(--border-subtle)] bg-[var(--surface-bg)] px-3.5 py-2.5 pl-9"
					>
						<div class="mb-2 flex items-center gap-1.5">
							<Wrench size={11} class="text-[var(--text-3)]" aria-hidden="true" />
							<span class="text-micro font-semibold uppercase tracking-wider text-[var(--text-3)]"
								>Tools</span
							>
							{#if toolSummary}
								<span class="text-micro text-[var(--text-faint)]">{toolSummary}</span>
							{/if}
						</div>
						{#if ledgerState === 'unavailable'}
							<p class="m-0 text-label italic text-[var(--text-faint)]">
								Tool calls unavailable — the session ledger failed to load.
							</p>
						{:else if ledgerState === 'pending'}
							<p class="m-0 text-label italic text-[var(--text-faint)]">Loading tool calls…</p>
						{:else if ledgerState === 'empty'}
							<p class="m-0 text-label italic text-[var(--text-faint)]">
								No tool calls in this turn.
							</p>
						{:else}
							<div class="flex flex-col">
								{#each visibleCalls as tool (tool.tool_id)}
									{@const status = statusFor(tool.request_id)}
									{@const running =
										!tool.result && (status === 'Pending' || status === 'Streaming')}
									{@const dim =
										(toolFilter !== 'all' && tool.tool_name !== toolFilter) ||
										(query.length > 0 && !toolCallMatchesQuery(tool, query))}
									<ToolCallCard
										{tool}
										requestNumber={requestNumberFor(tool.request_id)}
										{maxBytes}
										{running}
										{dim}
										forceOpen={tool.request_id === jumpTargetRequestId}
										onOpenDrawer={(kind) => onOpenDrawer(tool.request_id, kind)}
									/>
								{/each}
							</div>
							{#if hasMoreTools}
								<span class="mt-2 inline-flex bg-[var(--surface-bg)]">
									<button
										type="button"
										class="tc-showall inline-flex items-center gap-1.5 px-2.5 py-1 text-label text-[var(--text-2)] hover:text-[var(--interactive-active-fg)]"
										data-augmented-ui="tr-clip bl-clip border"
										onclick={() => (showAllTools = true)}
									>
										<ChevronDown size={12} aria-hidden="true" />
										Show all {toolCalls.length} tool calls
									</button>
								</span>
							{/if}
						{/if}
					</div>

					{#if workflowRequestIds.length > 0}
						<div
							class="border-b border-[var(--border-subtle)] bg-[var(--surface-bg)] px-3.5 py-2.5 pl-9"
						>
							<div class="mb-2 flex items-center gap-1.5">
								<Workflow size={11} class="text-[var(--text-3)]" aria-hidden="true" />
								<span class="text-micro font-semibold uppercase tracking-wider text-[var(--text-3)]"
									>Workflow runs</span
								>
							</div>
							<div class="flex flex-col">
								{#each workflowRequestIds as reqId (reqId)}
									<WorkflowRunBand sessionId={turn.session_id} parentRequestId={reqId} {now} />
								{/each}
							</div>
						</div>
					{/if}

					<div class="px-3.5 py-2.5 pl-9">
						<div
							class="mb-2 flex items-center gap-1.5 text-micro font-semibold uppercase tracking-wider"
						>
							<CornerDownRight size={11} class="text-[var(--tc-accent)]" aria-hidden="true" />
							<span style="color: var(--tc-accent);">{live ? 'Latest output' : 'Reply'}</span>
							<Button
								variant="outline"
								size="sm"
								class="ml-auto"
								onclick={() =>
									onOpenDrawer(turn.stop_request_id ?? turn.end_request_id, 'response')}
								>view response</Button
							>
						</div>
						{#if reply.show}
							<div class="tc-reply text-[var(--text-1)]">
								{#if query.length === 0}
									<MarkdownText text={reply.text} />
								{:else}
									<p class="m-0 whitespace-pre-wrap break-words">
										{#each highlightSegments(reply.text, query) as seg, i (i)}
											{#if seg.mark}<mark
													class="bg-[var(--interactive-active-bg)] text-[var(--interactive-active-fg)]"
													>{seg.text}</mark
												>{:else}{seg.text}{/if}
										{/each}
									</p>
								{/if}
								{#if reply.interrupted}
									<Badge variant="chip-error" class="mt-2">interrupted</Badge>
								{/if}
							</div>
						{:else}
							<p class="m-0 text-label italic text-[var(--text-faint)]">No reply captured yet.</p>
						{/if}
					</div>

					<div class="border-t border-[var(--border-subtle)] px-3.5 py-2.5 pl-9">
						<GoldLabelControl
							subject={{ kind: 'turn', turnId: turn.turn_id }}
							sessionId={turn.session_id}
							axes={['reaction', 'delivery']}
							context="turn_card"
						/>
					</div>
				</div>
			</div>
		{:else}
			<button
				type="button"
				class="flex w-full items-start gap-2 border-t border-[var(--border-subtle)] px-3.5 py-2 pl-9 text-left hover:bg-[var(--surface-2)]"
				aria-label={`Expand operator turn ${index} (reply preview)`}
				onclick={toggle}
			>
				<CornerDownRight
					size={11}
					class="mt-0.5 shrink-0 text-[var(--tc-accent)]"
					aria-hidden="true"
				/>
				<p class="tc-reply-preview m-0 text-label leading-normal text-[var(--text-3)]">
					{replyPreview.first.length > 0 ? replyPreview.first : '(no reply yet)'}
				</p>
			</button>
		{/if}
	</article>
</div>

<style>
	.tc {
		position: relative;
		--edge: 8px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--tc-accent);
		background: var(--surface-card);
		overflow: clip;
		box-shadow: inset 0 1px 0 var(--surface-shine);
	}

	.tc-body {
		display: grid;
	}
	.tc-body__inner {
		min-height: 0;
		overflow: hidden;
	}
	.tc-header {
		position: sticky;
		top: calc(44px + var(--timeline-cmdbar-h, 0px));
		z-index: 10;
		background: var(--surface-card);
		border-bottom: 1px solid var(--border-subtle);
	}
	.tc::after {
		z-index: 11;
		pointer-events: none;
	}
	.tc-prompt-preview {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.tc-reply-preview {
		display: block;
		max-height: calc(2 * 1.6 * var(--text-label, 11px));
		overflow: hidden;
	}
	.tc-reply {
		max-width: 92ch;
		font-size: var(--text-sm);
		line-height: 1.6;
		overflow-wrap: anywhere;
	}
	.tc-showall {
		--edge: 5px;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
	}
	.tc-showall:hover {
		--aug-border-bg: var(--interactive-active-border);
	}
	.tc--flash {
		animation: tc-flash-fade var(--dur-3) var(--ease-io) both;
	}
	@keyframes tc-flash-fade {
		0% {
			background: color-mix(in srgb, var(--tc-accent) 16%, transparent);
		}
		100% {
			background: transparent;
		}
	}
</style>
