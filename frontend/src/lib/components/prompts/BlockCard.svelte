<script lang="ts">
	import CopyIconButton from '$lib/components/shared/CopyIconButton.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import MarkdownIt from 'markdown-it';
	import DOMPurify from 'dompurify';
	import { Eye, Type } from '@lucide/svelte';

	export type BlockBadge = 'volatile' | 'authored' | 'draft' | null;

	interface Props {
		label: string;
		text: string;
		badge?: BlockBadge;
		editable?: boolean;
		railRole?: 'volatile' | 'preamble' | 'skeleton';
		expandable?: boolean;
	}

	let {
		label,
		text = $bindable(),
		badge = null,
		editable = false,
		railRole = 'preamble',
		expandable = false
	}: Props = $props();

	const charCount = $derived(text.length);
	const tokenEstimate = $derived(Math.ceil(charCount / 4));

	const railTooltip = $derived.by(() => {
		switch (railRole) {
			case 'volatile':
				return 'Volatile block — billing header injected by the harness, normalised before hashing.';
			case 'skeleton':
				return 'Skeleton + role overlay — block[2], typically the operator-supplied identity/instructions.';
			case 'preamble':
			default:
				return 'Identity preamble — block[1], common framing shared across compositions.';
		}
	});

	const badgeTooltip = $derived.by(() => {
		switch (badge) {
			case 'volatile':
				return 'Volatile block — bytes change per request and are normalised out of the prompt class hash.';
			case 'authored':
				return 'Authored block — operator-supplied content registered with the composition.';
			case 'draft':
				return 'Draft block — local edits not yet saved to the composition.';
			default:
				return null;
		}
	});

	const railVar = $derived(
		railRole === 'volatile'
			? 'var(--status-danger-bg)'
			: railRole === 'skeleton'
				? 'var(--status-warning-bg)'
				: 'var(--border-default)'
	);

	const badgeClass = $derived.by(() => {
		switch (badge) {
			case 'volatile':
				return 'badge-nx badge-muted';
			case 'authored':
				return 'badge-nx badge-active';
			case 'draft':
				return 'badge-nx badge-warning';
			default:
				return '';
		}
	});

	const showBadgeDot = $derived(badge === 'authored' || badge === 'draft');

	const charsLabel = $derived(`${charCount.toLocaleString('en-US')} chars`);
	const tokensLabel = $derived(`~${tokenEstimate.toLocaleString('en-US')} tokens`);

	const bodyAccessibleName = $derived(`${label} block content`);

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
	const renderedMarkdown = $derived(text.length === 0 ? '' : DOMPurify.sanitize(md.render(text)));
	const canTogglePreview = $derived(!editable && text.length > 0);
</script>

<article
	class="block-card flex bg-[var(--surface-card)] border-l-[3px] border-l-[var(--block-rail-color)]"
	data-augmented-ui="tr-clip br-clip border"
	style:--block-rail-color={railVar}
	data-slot="prompt-block-card"
	data-rail-role={railRole}
>
	<div class="flex min-w-0 flex-1 flex-col">
		<header
			class="flex items-center gap-3 border-b border-[var(--border-subtle)] px-3 py-2"
			data-slot="block-card-header"
		>
			<Tooltip text={railTooltip} slug="system-block">
				<span class="font-mono text-label text-[var(--text-2)]">{label}</span>
			</Tooltip>

			{#if badge && badgeTooltip}
				<Tooltip text={badgeTooltip}>
					<span class={badgeClass}>
						{#if showBadgeDot}
							<span class="badge-nx-dot"></span>
						{/if}
						{badge}
					</span>
				</Tooltip>
			{:else if badge}
				<span class={badgeClass}>
					{#if showBadgeDot}
						<span class="badge-nx-dot"></span>
					{/if}
					{badge}
				</span>
			{/if}

			<Tooltip
				text="Character count and approximate token count (≈4 chars / token)."
				slug="token-estimate"
			>
				<span class="ml-auto font-mono text-label text-[var(--text-3)]">
					{charsLabel} · {tokensLabel}
				</span>
			</Tooltip>

			{#if canTogglePreview}
				<button
					type="button"
					class="markdown-toggle inline-flex items-center gap-1 rounded border border-[var(--border-default)] px-2 py-1 text-label text-[var(--text-2)] outline-none transition-colors hover:border-[var(--interactive-active-border)] hover:text-[var(--interactive-active-fg)] focus-visible:ring-1 focus-visible:ring-[var(--interactive-active-border)]"
					aria-pressed={showMarkdown}
					title={showMarkdown ? 'Show plain text' : 'Render as markdown'}
					onclick={() => (showMarkdown = !showMarkdown)}
				>
					{#if showMarkdown}
						<Type class="size-3" aria-hidden="true" />
						<span>text</span>
					{:else}
						<Eye class="size-3" aria-hidden="true" />
						<span>md</span>
					{/if}
				</button>
			{/if}

			<CopyIconButton getText={() => text} label="Copy block text" copiedLabel="Copied" />
		</header>

		<div class="block-card-body px-3 py-3">
			{#if !editable && showMarkdown}
				<div
					class="block-markdown markdown-prose w-full rounded border border-[var(--border-default)] bg-[var(--surface-bg)] p-3 font-mono text-sm text-[var(--text-1)]"
				>
					{@html renderedMarkdown}
				</div>
			{:else if editable}
				<textarea
					class="block-textarea w-full resize-none rounded border border-[var(--border-default)] bg-[var(--surface-bg)] p-3 font-mono text-sm text-[var(--text-1)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--ring)]"
					class:block-textarea--expandable={expandable}
					aria-label={bodyAccessibleName}
					bind:value={text}
					rows={expandable ? Math.max(8, Math.min(120, text.split('\n').length + 1)) : undefined}
				></textarea>
			{:else}
				<textarea
					class="block-textarea w-full resize-none rounded border border-[var(--border-default)] bg-[var(--surface-bg)] p-3 font-mono text-sm text-[var(--text-1)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--ring)]"
					class:block-textarea--expandable={expandable}
					aria-label={bodyAccessibleName}
					readonly
					rows={expandable ? Math.max(8, Math.min(120, text.split('\n').length + 1)) : undefined}
					value={text}></textarea>
			{/if}
		</div>
	</div>
</article>

<style>
	.block-card {
		--edge: 8px;
		--aug-tr: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(135deg, var(--data-teal-fg) 0%, var(--border-default) 55%);
	}

	.block-textarea {
		max-height: calc(100vh - 320px);
		min-height: 4rem;
		overflow-y: auto;
		line-height: 1.5;
	}

	.block-textarea--expandable {
		max-height: none;
		overflow-y: visible;
	}

	.block-markdown {
		max-height: calc(100vh - 320px);
		min-height: 4rem;
		overflow-y: auto;
		line-height: 1.5;
		white-space: normal;
	}

	.block-markdown :global(p) {
		margin: 0 0 0.75rem 0;
	}
	.block-markdown :global(p:last-child) {
		margin-bottom: 0;
	}
	.block-markdown :global(ul),
	.block-markdown :global(ol) {
		margin: 0 0 0.75rem 1.5rem;
	}
	.block-markdown :global(code) {
		background: var(--bg-elevated);
		padding: 0 0.25rem;
		border-radius: 2px;
	}
	.block-markdown :global(pre) {
		background: var(--bg-elevated);
		padding: 0.5rem 0.75rem;
		border-radius: 4px;
		overflow-x: auto;
		margin: 0 0 0.75rem 0;
	}
	.block-markdown :global(pre code) {
		background: transparent;
		padding: 0;
	}
</style>
