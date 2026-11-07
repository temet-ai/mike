<script lang="ts">
	import { Dialog as DialogPrimitive } from 'bits-ui';
	import type { Component } from 'svelte';
	import { X, ArrowUpRight } from '@lucide/svelte';
	import * as Dialog from '$lib/components/ui/dialog';
	import CopyIconButton from '$lib/components/shared/CopyIconButton.svelte';
	import MarkdownText from '$lib/components/shared/MarkdownText.svelte';
	import type { IdentityFamily } from '$lib/board/types';

	let {
		open = $bindable(false),
		identityFamily,
		icon: Icon,
		titleLabel,
		name,
		heading,
		note,
		text,
		footerNote,
		openLabel = null,
		onOpenDispatch = null
	}: {
		open: boolean;
		identityFamily: IdentityFamily;
		icon: Component<{ size?: number | string }>;
		titleLabel: string;
		name: string;
		heading: string;
		note: string;
		text: string;
		footerNote: string;
		openLabel?: string | null;
		onOpenDispatch?: (() => void) | null;
	} = $props();

	let raw = $state(false);
	$effect(() => {
		if (!open) raw = false;
	});
</script>

<Dialog.Root bind:open>
	<Dialog.Portal>
		<Dialog.Overlay />
		<DialogPrimitive.Content
			data-slot="dialog-content"
			data-augmented-ui="tl-clip br-clip border"
			class="fpm-content fixed top-1/2 left-1/2 z-50 -translate-x-1/2 -translate-y-1/2 outline-none"
			style="--ident-fg: var(--ident-{identityFamily}-fg); --ident-fill: var(--ident-{identityFamily}-fill); --ident-line: var(--ident-{identityFamily}-line); --ident-plate: var(--ident-{identityFamily}-plate);"
		>
			<div class="fpm-head">
				<span class="fpm-plate" aria-hidden="true"><Icon size={16} /></span>
				<div class="fpm-headtext">
					<span class="fpm-kind">{titleLabel} · {name}</span>
					<Dialog.Title class="fpm-title">{heading}</Dialog.Title>
					<Dialog.Description class="sr-only">{note}</Dialog.Description>
				</div>
				<span class="fpm-note">{note}</span>
				<CopyIconButton getText={() => text} label="Copy" />
				<button
					type="button"
					class="fpm-btn"
					aria-pressed={raw}
					data-active={raw || undefined}
					onclick={() => (raw = !raw)}
				>
					Raw
				</button>
				<Dialog.Close class="fpm-btn fpm-close" aria-label="Close">
					<X size={11} aria-hidden="true" />
				</Dialog.Close>
			</div>

			<div class="fpm-body">
				{#if raw}
					<pre class="fpm-raw">{text}</pre>
				{:else}
					<div class="fpm-md"><MarkdownText {text} /></div>
				{/if}
			</div>

			<div class="fpm-foot">
				<span class="fpm-foot__note">{footerNote} · Esc or click outside to close</span>
				{#if openLabel && onOpenDispatch}
					<button type="button" class="fpm-btn" onclick={onOpenDispatch}>
						Open {openLabel}
						<ArrowUpRight size={11} aria-hidden="true" />
					</button>
				{/if}
			</div>
		</DialogPrimitive.Content>
	</Dialog.Portal>
</Dialog.Root>

<style>
	:global([data-slot='dialog-content'].fpm-content) {
		width: min(860px, calc(100vw - 2 * var(--space-4)));
		max-width: none;
		padding: 0;
		display: flex;
		flex-direction: column;
		max-height: min(1100px, calc(100dvh - 2 * var(--space-4)));
		background:
			linear-gradient(
				105deg,
				color-mix(in oklab, var(--ident-fg) 10%, transparent),
				transparent 30%
			),
			var(--surface-card);
		--aug-tl: var(--edge-md, 10px);
		--aug-br: var(--edge-md, 10px);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			color-mix(in oklab, var(--ident-fg) 60%, transparent),
			var(--border-default) 50%
		);
	}
	.fpm-head {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		padding: var(--space-3) var(--space-4);
		border-bottom: var(--hairline) solid var(--border-subtle);
	}
	.fpm-plate {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		flex: 0 0 auto;
		width: 32px;
		height: 32px;
		background: var(--ident-fill);
		color: var(--ident-fg);
		box-shadow: inset 0 0 0 1px var(--ident-line);
	}
	.fpm-headtext {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
		flex: 1 1 auto;
	}
	.fpm-kind {
		font-family: var(--font-mono);
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--ident-fg);
	}
	:global(.fpm-title) {
		font-family: var(--font-display) !important;
		font-size: 18px !important;
		font-weight: 700 !important;
		line-height: 1.2 !important;
		color: var(--text-strong) !important;
		overflow-wrap: anywhere;
	}
	.fpm-note {
		flex: 0 0 auto;
		font-family: var(--font-mono);
		font-size: 10px;
		color: var(--text-3);
		text-align: right;
		white-space: nowrap;
	}
	:global(.fpm-btn) {
		--edge: 4px;
		display: inline-flex;
		align-items: center;
		gap: 5px;
		flex: 0 0 auto;
		height: 26px;
		padding: 0 8px;
		border: 0;
		background: var(--surface-2);
		color: var(--text-2);
		font-family: var(--font-mono);
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		cursor: pointer;
	}
	:global(.fpm-btn:hover) {
		background: var(--interactive-hover-surface);
		color: var(--text-strong);
	}
	:global(.fpm-btn:focus-visible) {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	:global(.fpm-btn[data-active]) {
		background: var(--data-teal-bg);
		color: var(--data-teal-fg);
	}
	:global(.fpm-btn.fpm-close) {
		width: 26px;
		padding: 0;
		justify-content: center;
	}
	.fpm-body {
		padding: var(--space-4) var(--space-5, 26px);
		overflow: auto;
	}
	.fpm-raw {
		margin: 0;
		font-family: var(--font-mono);
		font-size: 12px;
		line-height: 1.6;
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.fpm-md {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		max-width: 72ch;
		font-family: var(--font-mono);
		font-size: 12.5px;
		line-height: 1.7;
		color: var(--text-1);
	}
	.fpm-md :global(.md-marker) {
		color: var(--ident-agent-fg);
	}
	.fpm-foot {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		padding: var(--space-3) var(--space-4);
		border-top: var(--hairline) solid var(--border-subtle);
	}
	.fpm-foot__note {
		font-family: var(--font-mono);
		font-size: 10px;
		color: var(--text-3);
		flex: 1 1 auto;
		min-width: 0;
	}
</style>
