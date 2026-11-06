<script lang="ts">
	import CompressionFlag from '$lib/components/compression/CompressionFlag.svelte';
	import type { CompressionTarget } from '$lib/api/compressionReview';
	import type { Snippet } from 'svelte';
	import { untrack } from 'svelte';
	import { AlertTriangle, Braces, ChevronDown, ChevronUp, Loader } from '@lucide/svelte';
	import { classify, KIND_PRESENTATION, parseMcpName, type ToolKind } from './toolRegistry';
	import BashCall from './BashCall.svelte';
	import ReadCall from './ReadCall.svelte';
	import EditCall from './EditCall.svelte';
	import WriteCall from './WriteCall.svelte';
	import FetchCall from './FetchCall.svelte';
	import SearchCall from './SearchCall.svelte';
	import TaskCall from './TaskCall.svelte';
	import WebSearchCall from './WebSearchCall.svelte';
	import TodoCall from './TodoCall.svelte';
	import McpCall from './McpCall.svelte';
	import GenericCall from './GenericCall.svelte';
	import ToolResultCard from './ToolResultCard.svelte';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import { cn } from '$lib/utils';
	import type { ToolUseDto } from '$lib/types';
	import {
		categoryOf,
		categoryColorVar,
		toolTarget,
		toolInputText,
		toolResultText,
		type ToolCategory
	} from '$lib/components/shell/timelineTurnView';

	interface LoadedContent {
		input: unknown;
		result: { content: unknown; is_error: boolean } | null;
	}

	interface Props {
		compressionTarget?: CompressionTarget;
		name?: string;
		input?: unknown;
		showRawInput?: boolean;
		rawOpen?: boolean;
		collapsible?: boolean;
		meta?: Snippet;
		result?: { content: unknown; isError?: boolean; toolName?: string };
		operation?: string | null;
		loadContent?: () => Promise<LoadedContent>;
		tool?: ToolUseDto;
		requestNumber?: number | null;
		maxBytes?: number;
		running?: boolean;
		dim?: boolean;
		forceOpen?: boolean;
		onOpenDrawer?: (kind: 'request' | 'response') => void;
	}

	let {
		compressionTarget,
		name,
		input,
		showRawInput = true,
		rawOpen = false,
		collapsible = false,
		meta,
		result,
		loadContent,
		operation = null,
		tool,
		requestNumber = null,
		maxBytes = 1,
		running = false,
		dim = false,
		forceOpen = false,
		onOpenDrawer
	}: Props = $props();

	const dense = $derived(tool !== undefined);
	const effectiveName = $derived(tool ? tool.tool_name : (name ?? ''));

	const kind = $derived<ToolKind>(classify(effectiveName));
	const presentation = $derived(KIND_PRESENTATION[kind]);
	const mcp = $derived(parseMcpName(effectiveName));

	const denseCategory = $derived<ToolCategory>(categoryOf(effectiveName));
	const denseAccentVar = $derived(categoryColorVar(denseCategory));
	const denseTarget = $derived(tool ? toolTarget(tool) : '');
	const denseIsError = $derived(tool?.result?.is_error === true);
	const denseReqLabel = $derived(requestNumber ?? tool?.request_id);
	const denseBytes = $derived(tool?.result?.content_bytes ?? null);
	const denseBarPct = $derived.by(() => {
		if (running) return 55;
		if (denseBytes === null) return 0;
		return Math.max(6, Math.round((denseBytes / Math.max(1, maxBytes)) * 100));
	});
	function formatBytes(n: number): string {
		if (n < 1024) return `${n} B`;
		if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
		return `${(n / (1024 * 1024)).toFixed(1)} MB`;
	}
	const denseBytesLabel = $derived(
		denseBytes === null ? (running ? '…' : '—') : formatBytes(denseBytes)
	);
	const denseInputText = $derived(tool ? toolInputText(tool) : '');
	const denseResultText = $derived(tool ? toolResultText(tool) : null);

	let cardExpanded = $state(untrack(() => (dense ? false : !collapsible)));
	$effect(() => {
		if (dense && forceOpen) cardExpanded = true;
	});

	type LazyState =
		| { kind: 'idle' }
		| { kind: 'loading' }
		| { kind: 'error'; message: string }
		| { kind: 'loaded'; content: LoadedContent };

	let lazyState = $state<LazyState>({ kind: 'idle' });

	function toggleCard(): void {
		if (!dense && !collapsible) return;
		cardExpanded = !cardExpanded;
		if (cardExpanded && loadContent && lazyState.kind === 'idle') {
			lazyState = { kind: 'loading' };
			loadContent()
				.then((c) => {
					lazyState = { kind: 'loaded', content: c };
				})
				.catch((err) => {
					lazyState = {
						kind: 'error',
						message: err instanceof Error ? err.message : String(err)
					};
				});
		}
	}

	const resolvedInput = $derived(
		dense
			? tool?.input
			: loadContent
				? lazyState.kind === 'loaded'
					? lazyState.content.input
					: undefined
				: input
	);

	const resolvedResult = $derived(
		(): { content: unknown; isError?: boolean; toolName?: string } | undefined => {
			if (dense) return undefined;
			if (loadContent) {
				if (lazyState.kind !== 'loaded' || lazyState.content.result === null) return undefined;
				return {
					toolName: effectiveName,
					content: lazyState.content.result.content,
					isError: lazyState.content.result.is_error
				};
			}
			return result;
		}
	);

	let rawExpanded = $state(untrack(() => rawOpen));
	function toggleRaw(e: Event): void {
		e.stopPropagation();
		rawExpanded = !rawExpanded;
	}

	function rawJson(value: unknown): string {
		try {
			return JSON.stringify(value, null, 2);
		} catch {
			return String(value);
		}
	}
</script>

{#if dense}
	<div
		class={cn(
			'rounded-[2px] border',
			denseIsError ? 'border-[var(--status-danger-border)]' : 'border-[var(--border-subtle)]',
			cardExpanded ? 'bg-[var(--surface-2)]' : 'bg-[var(--surface-card)]'
		)}
		style="opacity: {dim ? 0.45 : 1};"
		data-motion="hover"
		data-slot="timeline-tool-call"
		data-request-id={tool?.request_id}
	>
		<button
			type="button"
			class="grid w-full grid-cols-[44px_26px_1fr_84px_18px] items-center gap-2.5 rounded-[2px] px-2.5 py-1.5 text-left hover:bg-[var(--surface-2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--interactive-focus-ring)]"
			aria-expanded={cardExpanded}
			aria-label={`${cardExpanded ? 'Collapse' : 'Expand'} ${effectiveName} call, request ${denseReqLabel}`}
			onclick={toggleCard}
		>
			<span class="font-mono text-micro tabular-nums text-[var(--text-faint)]"
				>#{denseReqLabel}</span
			>
			<span
				class="flex size-[22px] shrink-0 items-center justify-center rounded-[2px]"
				style="background: color-mix(in srgb, var({denseAccentVar}) 16%, transparent); color: var({denseAccentVar});"
			>
				<presentation.icon size={13} aria-hidden="true" />
			</span>
			<span class="flex min-w-0 items-baseline gap-2">
				<span class="shrink-0 font-mono text-xs font-medium text-[var(--text-strong)]"
					>{effectiveName}</span
				>
				{#if denseTarget}
					<span class="min-w-0 break-words font-mono text-xs text-[var(--text-2)]"
						>{denseTarget}</span
					>
				{/if}
			</span>
			<span class="flex flex-col items-end gap-1">
				<span class="font-mono text-micro tabular-nums text-[var(--text-2)]">{denseBytesLabel}</span
				>
				<span class="h-[3px] w-[60px] overflow-hidden bg-[var(--surface-3)]">
					<span
						class="block h-full"
						style="width: {denseBarPct}%; background: {denseIsError
							? 'var(--status-danger-fg)'
							: `var(${denseAccentVar})`};"
					></span>
				</span>
			</span>
			<span class="flex justify-center text-[var(--text-faint)]">
				{#if denseIsError}
					<AlertTriangle size={13} class="text-[var(--status-danger-fg)]" aria-hidden="true" />
				{:else if running}
					<Loader size={13} data-motion="wait wait-spin" aria-hidden="true" />
				{:else if cardExpanded}
					<ChevronUp size={13} aria-hidden="true" />
				{:else}
					<ChevronDown size={13} aria-hidden="true" />
				{/if}
			</span>
		</button>

		{#if cardExpanded}
			<div
				class="flex flex-col gap-2 border-t border-[var(--border-subtle)] bg-[var(--surface-bg)] px-3 py-2 pl-[76px]"
			>
				{#if denseInputText.length > 0}
					<div>
						<div class="mb-1 text-micro font-semibold tracking-kpi text-[var(--text-faint)]">
							INPUT
						</div>
						<pre
							class="max-h-[220px] overflow-auto whitespace-pre-wrap break-words font-mono text-label leading-relaxed text-[var(--text-2)]">{denseInputText}</pre>
					</div>
				{/if}
				<div>
					<div
						class={cn(
							'mb-1 text-micro font-semibold tracking-kpi',
							denseIsError ? 'text-[var(--status-danger-fg)]' : 'text-[var(--text-faint)]'
						)}
					>
						{denseIsError ? 'ERROR' : 'RESULT'}
					</div>
					{#if denseResultText === null}
						<p class="font-mono text-label italic text-[var(--text-faint)]">
							{running ? 'running…' : 'no result captured'}
						</p>
					{:else}
						{#if tool?.use_id && tool.result?.request_id}<CompressionFlag
								target={{
									kind: 'tool_result',
									request_id: tool.result.request_id,
									tool_use_id: tool.use_id
								}}
							/>{/if}
						<pre
							class={cn(
								'max-h-[280px] overflow-auto whitespace-pre-wrap break-words font-mono text-label leading-relaxed',
								denseIsError ? 'text-[var(--status-danger-fg)]' : 'text-[var(--text-1)]'
							)}>{denseResultText}</pre>
					{/if}
				</div>
				{#if onOpenDrawer}
					<div class="flex gap-3 pt-0.5">
						<button
							type="button"
							class="text-micro uppercase tracking-wider text-[var(--text-faint)] hover:text-[var(--text-strong)]"
							onclick={(e) => {
								e.stopPropagation();
								onOpenDrawer?.('request');
							}}>view request</button
						>
						<button
							type="button"
							class="text-micro uppercase tracking-wider text-[var(--text-faint)] hover:text-[var(--text-strong)]"
							onclick={(e) => {
								e.stopPropagation();
								onOpenDrawer?.('response');
							}}>view response</button
						>
					</div>
				{/if}
			</div>
		{/if}
	</div>
{:else}
	<article
		class="tool-card"
		class:tool-card--collapsible={collapsible}
		data-augmented-ui="tr-clip br-clip border"
		style="--accent: var({presentation.accentVar});"
	>
		{#if collapsible}
			<div class="tool-card__header-row">
				<button
					type="button"
					class="tool-card__header tool-card__header--toggle"
					aria-expanded={cardExpanded}
					aria-label={cardExpanded
						? `Collapse ${effectiveName} tool call`
						: `Expand ${effectiveName} tool call`}
					onclick={toggleCard}
				>
					<DisclosureMarker open={cardExpanded} size={12} />
					<span class="tool-card__icon" aria-hidden="true">
						<presentation.icon size={14} />
					</span>
					<span class="tool-card__name">{effectiveName}</span>
					{#if mcp !== null}
						<span class="tool-card__kind-badge">{mcp.server}</span>
					{/if}
					{#if operation}
						<span class="tool-card__op-badge">{operation}</span>
					{/if}
				</button>
				{#if meta}
					<span class="tool-card__meta">
						{@render meta()}
					</span>
				{/if}
			</div>
		{:else}
			<header class="tool-card__header">
				<span class="tool-card__icon" aria-hidden="true">
					<presentation.icon size={14} />
				</span>
				<span class="tool-card__name">{effectiveName}</span>
				{#if mcp !== null}
					<span class="tool-card__kind-badge">{mcp.server}</span>
				{/if}
				{#if operation}
					<span class="tool-card__op-badge">{operation}</span>
				{/if}
			</header>
		{/if}

		{#if cardExpanded}
			{#if loadContent && lazyState.kind === 'loading'}
				<div class="tool-card__lazy-loading">Loading…</div>
			{:else if loadContent && lazyState.kind === 'error'}
				<div class="tool-card__lazy-error">Couldn't load: {lazyState.message}</div>
			{:else}
				<div class="tool-card__body">
					{#if kind === 'bash'}
						<BashCall input={resolvedInput} />
					{:else if kind === 'read'}
						<ReadCall input={resolvedInput} />
					{:else if kind === 'edit'}
						<EditCall input={resolvedInput} />
					{:else if kind === 'write'}
						<WriteCall input={resolvedInput} />
					{:else if kind === 'fetch'}
						<FetchCall input={resolvedInput} />
					{:else if kind === 'search'}
						<SearchCall input={resolvedInput} />
					{:else if kind === 'task'}
						<TaskCall input={resolvedInput} />
					{:else if kind === 'websearch'}
						<WebSearchCall input={resolvedInput} />
					{:else if kind === 'todo'}
						<TodoCall input={resolvedInput} />
					{:else if kind === 'mcp'}
						<McpCall name={effectiveName} input={resolvedInput} />
					{:else}
						<GenericCall input={resolvedInput} />
					{/if}
				</div>

				{#if showRawInput}
					<div class="tool-card__raw">
						<button
							type="button"
							class="tool-card__raw-toggle"
							aria-expanded={rawExpanded}
							onclick={toggleRaw}
						>
							<DisclosureMarker open={rawExpanded} size={12} />
							<Braces size={12} aria-hidden="true" class="raw-braces" />
							<span>{rawExpanded ? 'Hide raw input' : 'Raw input'}</span>
						</button>
						{#if rawExpanded}
							<div class="tool-card__raw-body" aria-label="Full tool input JSON">
								<pre class="tool-card__raw-json">{rawJson(resolvedInput)}</pre>
							</div>
						{/if}
					</div>
				{/if}

				{#if resolvedResult() !== undefined}
					{@const r = resolvedResult()!}
					{#if compressionTarget}<CompressionFlag target={compressionTarget} />{/if}
					<ToolResultCard
						toolName={r.toolName ?? effectiveName}
						content={typeof r.content === 'string' ? r.content : JSON.stringify(r.content, null, 2)}
						isError={r.isError}
						{rawOpen}
					/>
				{/if}
			{/if}
		{/if}
	</article>
{/if}

<style>
	.tool-card {
		--edge: 8px;
		--aug-tr: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(135deg, var(--accent) 0%, var(--border-default) 60%);
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 10px calc(var(--edge) + 4px) 10px 12px;
		background: var(--surface-card);
		border: 1px solid var(--border-subtle);
		border-left: 3px solid var(--accent);
	}

	.tool-card--collapsible {
		padding: 0;
		gap: 0;
	}

	.tool-card__header-row {
		display: flex;
		align-items: center;
	}

	.tool-card__header {
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
	}

	.tool-card__header--toggle {
		flex: 1 1 0;
		min-width: 0;
		padding: 8px calc(var(--edge) + 4px) 8px 10px;
		background: none;
		border: none;
		cursor: pointer;
		text-align: left;
		color: inherit;
		font: inherit;
		border-radius: 2px 2px 0 0;
	}

	.tool-card__header--toggle:hover {
		background: color-mix(in srgb, var(--surface-card) 80%, var(--border-subtle) 20%);
	}

	.tool-card__header--toggle:focus-visible {
		outline: none;
		box-shadow: inset 0 0 0 2px var(--interactive-focus-ring);
	}

	.tool-card__icon {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		color: var(--accent);
		flex-shrink: 0;
	}
	.tool-card__name {
		font-family: var(--font-mono);
		font-size: 13px;
		font-weight: 600;
		color: var(--accent);
		word-break: break-all;
		overflow-wrap: anywhere;
		min-width: 0;
	}
	.tool-card__kind-badge {
		flex-shrink: 0;
		padding: 1px 7px;
		border: 1px solid var(--border-default);
		border-radius: 2px;
		background: var(--surface-2);
		color: var(--text-3);
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.03em;
	}

	.tool-card__op-badge {
		flex-shrink: 0;
		padding: 1px 7px;
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		background: color-mix(in srgb, var(--accent) 10%, var(--surface-2) 90%);
		color: var(--accent);
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.03em;
	}

	.tool-card__meta {
		flex-shrink: 0;
		padding: 0 calc(var(--edge) + 4px) 0 10px;
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.tool-card--collapsible .tool-card__body {
		padding: 0 calc(var(--edge) + 4px) 6px 10px;
	}

	.tool-card__body {
		min-width: 0;
	}
	.tool-card__raw {
		display: flex;
		flex-direction: column;
		gap: 6px;
		border-top: 1px solid var(--border-subtle);
		padding-top: 8px;
	}

	.tool-card--collapsible .tool-card__raw {
		padding: 8px calc(var(--edge) + 4px) 10px 10px;
	}

	.tool-card__raw-toggle {
		display: inline-flex;
		align-items: center;
		align-self: flex-start;
		gap: 6px;
		padding: 0;
		background: none;
		border: none;
		cursor: pointer;
		color: var(--text-3);
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}
	.tool-card__raw-toggle:hover {
		color: var(--text-strong);
	}
	.tool-card__raw-toggle:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
		border-radius: 2px;
	}
	.tool-card__raw-toggle :global(.raw-braces) {
		color: var(--text-faint);
	}
	.tool-card__lazy-loading,
	.tool-card__lazy-error {
		padding: 10px 12px;
		font-size: 12px;
		font-style: italic;
		color: var(--text-3);
	}
	.tool-card__lazy-error {
		color: var(--status-danger-fg);
		font-style: normal;
	}

	.tool-card__raw-body {
		background: var(--surface-code);
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		max-height: 360px;
		overflow: auto;
	}
	.tool-card__raw-json {
		margin: 0;
		padding: 10px;
		font-family: var(--font-mono);
		font-size: var(--text-label);
		line-height: 1.6;
		color: var(--text-1);
		white-space: pre;
		word-break: normal;
	}
</style>
