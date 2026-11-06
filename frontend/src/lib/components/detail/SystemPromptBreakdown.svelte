<script lang="ts">
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import type { HTMLAttributes } from 'svelte/elements';
	import MarkdownIt from 'markdown-it';
	import DOMPurify from 'dompurify';
	import { cn, type WithElementRef } from '$lib/utils';
	import { formatCount } from '$lib/time';
	import { formatTokens } from '$lib/components/message-view/parseSystemPrompt';
	import { Check, Copy, FileText, Filter } from '@lucide/svelte';
	import SubRow from '$lib/components/message-view/SubRow.svelte';
	import SystemPromptVisibilitySignal from '$lib/components/detail/SystemPromptVisibilitySignal.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import type { SystemBlock, SystemBlocksProjection } from '$lib/types';

	interface Props extends WithElementRef<HTMLAttributes<HTMLDivElement>> {
		blocks: SystemBlock[] | null;
		hoistHeaderActions?: boolean;
		headerActionsApi?: HeaderActionApi | null;
		systemBlocks?: SystemBlocksProjection | null;
		requestId?: string;
	}

	export interface HeaderActionApi {
		showMarkdown: boolean;
		toggleMarkdown: () => void;
		canCopy: boolean;
		copied: boolean;
		copy: () => Promise<void>;
	}

	let {
		ref = $bindable(null),
		class: className,
		blocks: inputBlocks,
		hoistHeaderActions = false,
		headerActionsApi = $bindable(null),
		systemBlocks = null,
		requestId = '',
		...restProps
	}: Props = $props();

	function estimateTokens(charCount: number): number {
		return Math.max(0, Math.round(charCount / 4));
	}

	interface Block {
		index: number;
		charCount: number;
		estTokens: number;
		fullText: string;
		composition_match: 'matched' | 'unknown' | 'extra' | null;
	}

	const blocks = $derived.by((): Block[] => {
		const raw = inputBlocks;
		if (raw === null || raw.length === 0) return [];
		return raw
			.map((block) => ({
				index: block.index,
				charCount: block.text.length,
				estTokens: estimateTokens(block.text.length),
				fullText: block.text,
				composition_match: block.composition_match
			}))
			.filter((b) => b.charCount > 0);
	});

	const joinedText = $derived(blocks.map((b) => b.fullText).join('\n\n'));

	const totalChars = $derived(blocks.reduce((acc, b) => acc + b.charCount, 0));
	const totalTokens = $derived(estimateTokens(totalChars));

	const md = new MarkdownIt({ html: false, linkify: false, breaks: false });

	const defaultLinkOpen =
		md.renderer.rules.link_open ??
		((tokens, idx, options, _env, self) => self.renderToken(tokens, idx, options));
	md.renderer.rules.link_open = (tokens, idx, options, env, self) => {
		const token = tokens[idx];
		if (token !== undefined) {
			const targetIdx = token.attrIndex('target');
			if (targetIdx < 0) token.attrPush(['target', '_blank']);
			else token.attrSet('target', '_blank');
			const relIdx = token.attrIndex('rel');
			if (relIdx < 0) token.attrPush(['rel', 'noopener noreferrer']);
			else token.attrSet('rel', 'noopener noreferrer');
		}
		return defaultLinkOpen(tokens, idx, options, env, self);
	};

	let showMarkdown = $state(false);

	const renderedMarkdown = $derived(
		joinedText.length === 0 ? '' : DOMPurify.sanitize(md.render(joinedText))
	);

	let clipboardSupported = $state(false);
	let copied = $state(false);
	let copyTimer: ReturnType<typeof setTimeout> | null = null;

	onMount(() => {
		clipboardSupported = typeof navigator !== 'undefined' && !!navigator.clipboard?.writeText;
		return () => {
			if (copyTimer !== null) clearTimeout(copyTimer);
		};
	});

	async function handleCopy(): Promise<void> {
		if (joinedText.length === 0) return;
		try {
			await navigator.clipboard.writeText(joinedText);
			copied = true;
			if (copyTimer !== null) clearTimeout(copyTimer);
			copyTimer = setTimeout(() => {
				copied = false;
				copyTimer = null;
			}, 1500);
		} catch {}
	}

	const canCopy = $derived(clipboardSupported && joinedText.length > 0);

	$effect(() => {
		if (!hoistHeaderActions) return;
		headerActionsApi = {
			showMarkdown,
			toggleMarkdown: () => (showMarkdown = !showMarkdown),
			canCopy,
			copied,
			copy: handleCopy
		};
	});
</script>

<div
	bind:this={ref}
	data-slot="system-prompt-breakdown"
	class={cn('flex flex-col gap-2', className)}
	{...restProps}
>
	{#if blocks.length === 0}
		<p class="text-xs text-[var(--text-3)]">No system prompt on this request.</p>
	{:else if showMarkdown}
		{#if !hoistHeaderActions}
			<header class="flex items-center justify-between gap-2 px-1">
				<span class="text-label font-mono tabular-nums text-[var(--text-2)]">
					{blocks.length} block{blocks.length === 1 ? '' : 's'} · {formatCount(totalChars)} chars · {formatTokens(
						totalTokens
					)}
				</span>
				<div class="flex items-center gap-2">
					<SystemPromptVisibilitySignal {systemBlocks} {requestId} />
					<div class="flex items-center gap-1">
						<button
							type="button"
							class="inline-flex size-6 items-center justify-center rounded-[2px] text-[var(--text-3)] transition-colors hover:bg-[var(--surface-card)] hover:text-[var(--text-strong)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--interactive-focus-ring)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--surface-bg)] aria-pressed:text-[var(--text-strong)]"
							title={showMarkdown ? 'Show breakdown' : 'Show as markdown'}
							aria-label={showMarkdown ? 'Show breakdown' : 'Show as markdown'}
							aria-pressed={showMarkdown}
							onclick={() => (showMarkdown = !showMarkdown)}
						>
							<FileText size={12} aria-hidden="true" />
						</button>
						{#if canCopy}
							<button
								type="button"
								class="inline-flex size-6 items-center justify-center rounded-[2px] text-[var(--text-3)] transition-colors hover:bg-[var(--surface-card)] hover:text-[var(--text-strong)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--interactive-focus-ring)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--surface-bg)]"
								title={copied ? 'Copied' : 'Copy'}
								aria-label={copied ? 'Copied' : 'Copy'}
								aria-live="polite"
								onclick={handleCopy}
							>
								{#if copied}
									<Check size={12} aria-hidden="true" />
								{:else}
									<Copy size={12} aria-hidden="true" />
								{/if}
							</button>
						{/if}
					</div>
				</div>
			</header>
		{/if}
		<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
		<div
			data-slot="system-prompt-markdown"
			role="region"
			tabindex="0"
			aria-label="System prompt markdown"
			class="markdown-body max-h-[600px] overflow-auto rounded-[2px] border border-[var(--border-default)] bg-[var(--surface-bg)] p-3 text-xs text-[var(--text-1)]"
		>
			<!-- eslint-disable-next-line svelte/no-at-html-tags -->
			{@html renderedMarkdown}
		</div>
	{:else}
		{#if !hoistHeaderActions}
			<header class="flex items-center justify-between gap-2 px-1">
				<span class="text-label font-mono tabular-nums text-[var(--text-2)]">
					{blocks.length} block{blocks.length === 1 ? '' : 's'} · {formatCount(totalChars)} chars · {formatTokens(
						totalTokens
					)}
				</span>
				<div class="flex items-center gap-2">
					<SystemPromptVisibilitySignal {systemBlocks} {requestId} />
					<div class="flex items-center gap-1">
						<button
							type="button"
							class="inline-flex size-6 items-center justify-center rounded-[2px] text-[var(--text-3)] transition-colors hover:bg-[var(--surface-card)] hover:text-[var(--text-strong)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--interactive-focus-ring)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--surface-bg)] aria-pressed:text-[var(--text-strong)]"
							title={showMarkdown ? 'Show breakdown' : 'Show as markdown'}
							aria-label={showMarkdown ? 'Show breakdown' : 'Show as markdown'}
							aria-pressed={showMarkdown}
							onclick={() => (showMarkdown = !showMarkdown)}
						>
							<FileText size={12} aria-hidden="true" />
						</button>
						{#if canCopy}
							<button
								type="button"
								class="inline-flex size-6 items-center justify-center rounded-[2px] text-[var(--text-3)] transition-colors hover:bg-[var(--surface-card)] hover:text-[var(--text-strong)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--interactive-focus-ring)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--surface-bg)]"
								title={copied ? 'Copied' : 'Copy'}
								aria-label={copied ? 'Copied' : 'Copy'}
								aria-live="polite"
								onclick={handleCopy}
							>
								{#if copied}
									<Check size={12} aria-hidden="true" />
								{:else}
									<Copy size={12} aria-hidden="true" />
								{/if}
							</button>
						{/if}
					</div>
				</div>
			</header>
		{:else if systemBlocks}
			<div class="flex items-center gap-2 px-1" data-slot="system-prompt-visibility-strip">
				<SystemPromptVisibilitySignal {systemBlocks} {requestId} />
			</div>
		{/if}
		<div data-slot="system-prompt-rows" class="tsc-rows">
			{#each blocks as block (block.index)}
				{@const matchState = block.composition_match}
				<SubRow class="group items-start">
					<Tooltip
						text={`System block ${block.index + 1} — captured slice of the system field.`}
						slug="system-block"
					>
						<span class="text-caption shrink-0 font-mono tabular-nums text-[var(--text-3)]">
							#{block.index + 1}
						</span>
					</Tooltip>
					<span class="flex-1 whitespace-pre-wrap break-words text-xs text-[var(--text-1)]">
						{block.fullText.trim() ? block.fullText : '(empty block)'}
					</span>
					{#if matchState === 'unknown'}
						<Tooltip
							text="Block drifted from composition — bytes differ from the registered shape."
							slug="composition-match"
						>
							<span class="badge-nx badge-warning shrink-0"> unknown </span>
						</Tooltip>
					{:else if matchState === 'extra'}
						<Tooltip
							text="Block present here but not in the registered composition."
							slug="composition-match"
						>
							<span class="badge-nx badge-error shrink-0"> extra block </span>
						</Tooltip>
					{/if}
					<Tooltip
						text="Character count and approximate token count (≈4 chars / token)."
						slug="token-estimate"
					>
						<span class="text-label shrink-0 font-mono tabular-nums text-[var(--text-2)]">
							{formatCount(block.charCount)} chars
						</span>
					</Tooltip>
					<span
						class="text-label w-28 shrink-0 text-right font-mono tabular-nums text-[var(--text-3)]"
					>
						{formatTokens(block.estTokens)}
					</span>
					{#if requestId}
						<a
							href={resolve(`/filter/workbench?from=request:${requestId}&block=${block.index}`)}
							aria-label={`Open filter workbench for block ${block.index + 1}`}
							title="Filter this block"
							data-slot="system-prompt-filter-this"
							class="inline-flex size-6 shrink-0 items-center justify-center rounded-[2px] text-[var(--text-3)] opacity-0 transition-all duration-150 hover:text-[var(--interactive-active-fg)] focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--interactive-focus-ring)] focus-visible:ring-offset-1 focus-visible:ring-offset-[var(--surface-bg)] group-hover:opacity-100"
						>
							<Filter size={12} aria-hidden="true" />
						</a>
					{/if}
				</SubRow>
			{/each}
		</div>
	{/if}
</div>

<style>
	.markdown-body :global(h1),
	.markdown-body :global(h2),
	.markdown-body :global(h3),
	.markdown-body :global(h4),
	.markdown-body :global(h5),
	.markdown-body :global(h6) {
		color: var(--text-strong);
		font-weight: 600;
		margin-top: 1rem;
		margin-bottom: 0.5rem;
	}
	.markdown-body :global(h1:first-child),
	.markdown-body :global(h2:first-child),
	.markdown-body :global(h3:first-child),
	.markdown-body :global(h4:first-child),
	.markdown-body :global(h5:first-child),
	.markdown-body :global(h6:first-child) {
		margin-top: 0;
	}
	.markdown-body :global(h1) {
		font-size: var(--text-lg);
	}
	.markdown-body :global(h2) {
		font-size: 0.9375rem;
	}
	.markdown-body :global(h3) {
		font-size: 0.875rem;
	}
	.markdown-body :global(h4),
	.markdown-body :global(h5),
	.markdown-body :global(h6) {
		font-size: 0.8125rem;
	}
	.markdown-body :global(p) {
		margin: 0.5rem 0;
	}
	.markdown-body :global(p:first-child) {
		margin-top: 0;
	}
	.markdown-body :global(p:last-child) {
		margin-bottom: 0;
	}
	.markdown-body :global(ul),
	.markdown-body :global(ol) {
		margin: 0.5rem 0;
		padding-left: 1.25rem;
	}
	.markdown-body :global(li) {
		margin: 0.125rem 0;
	}
	.markdown-body :global(li > p) {
		margin: 0.125rem 0;
	}
	.markdown-body :global(code) {
		background: var(--surface-2);
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		color: var(--text-strong);
		font-family: inherit;
		padding: 0 0.25rem;
	}
	.markdown-body :global(pre) {
		background: var(--surface-card);
		border: 1px solid var(--border-default);
		border-radius: 2px;
		color: var(--text-strong);
		margin: 0.5rem 0;
		overflow: auto;
		padding: 0.5rem;
	}
	.markdown-body :global(pre code) {
		background: transparent;
		border: 0;
		color: inherit;
		padding: 0;
	}
	.markdown-body :global(blockquote) {
		border-left: 3px solid var(--border-default);
		color: var(--text-2);
		margin: 0.5rem 0;
		padding: 0.125rem 0 0.125rem 0.75rem;
	}
	.markdown-body :global(a) {
		color: var(--status-info-fg);
		text-decoration: underline;
	}
	.markdown-body :global(a:hover) {
		color: var(--text-strong);
	}
	.markdown-body :global(strong) {
		color: var(--text-strong);
		font-weight: 600;
	}
	.markdown-body :global(em) {
		font-style: italic;
	}
	.markdown-body :global(hr) {
		border: 0;
		border-top: 1px solid var(--border-default);
		margin: 0.75rem 0;
	}

	:global(.tsc-rows .subrow.items-start) {
		align-items: flex-start;
	}
</style>
