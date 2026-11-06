<script module lang="ts">
	export interface SessionPreviewNode {
		id: string;
		title: string;
		outcome: string | null;
		similarity: number;
	}

	export type SessionFetcher = (id: string, params?: GetSessionParams) => Promise<SessionDetail>;
</script>

<script lang="ts">
	import { Dialog as DialogPrimitive } from 'bits-ui';
	import * as Dialog from '$lib/components/ui/dialog';
	import { X } from '@lucide/svelte';
	import { onDestroy, untrack } from 'svelte';
	import Button from '$lib/components/ui/button/button.svelte';
	import { getSession, ApiError, type GetSessionParams } from '$lib/api';
	import type { SessionDetail } from '$lib/types';
	import { graphOutcomeColorVar, graphOutcomeLabel } from './outcomeColors';
	import { formatCount, formatCompactCount, formatRelative, formatClockAbsolute } from '$lib/time';

	interface Props {
		node: SessionPreviewNode;
		originEl: HTMLElement;
		onClose: () => void;
		onOpenSession: (sessionId: string) => void;
		fetchSession?: SessionFetcher;
	}
	let { node, originEl, onClose, onOpenSession, fetchSession = getSession }: Props = $props();

	let contentEl: HTMLElement | null = $state(null);
	let materialized = $state(false);
	let closing = $state(false);

	function rectOf(el: HTMLElement): { top: number; left: number; width: number; height: number } {
		const r = el.getBoundingClientRect();
		return { top: r.top, left: r.left, width: r.width, height: r.height };
	}

	function applyRect(
		el: HTMLElement,
		rect: { top: number; left: number; width: number; height: number }
	): void {
		el.style.top = `${rect.top}px`;
		el.style.left = `${rect.left}px`;
		el.style.width = `${rect.width}px`;
		el.style.height = `${rect.height}px`;
	}

	let closeTimer: ReturnType<typeof setTimeout> | undefined;
	let animationFrame: number | undefined;

	function centerPreview(el: HTMLElement): void {
		const viewport = window.visualViewport;
		const vw = viewport?.width ?? window.innerWidth;
		const vh = viewport?.height ?? window.innerHeight;
		const gap = parseFloat(getComputedStyle(el).getPropertyValue('--space-4')) || 16;
		const width = Math.min(440, vw - gap * 2);
		const height = Math.min(420, vh - gap * 2);
		applyRect(el, {
			top: (viewport?.offsetTop ?? 0) + (vh - height) / 2,
			left: (viewport?.offsetLeft ?? 0) + (vw - width) / 2,
			width,
			height
		});
	}

	$effect(() => {
		const el = contentEl;
		if (!el) return;
		untrack(() => {
			el.style.transition = 'none';
			applyRect(el, rectOf(originEl));
			el.getBoundingClientRect();
			animationFrame = requestAnimationFrame(() => {
				el.style.transition = '';
				materialized = true;
				centerPreview(el);
			});
		});
		const resize = () => {
			if (!closing) centerPreview(el);
		};
		window.addEventListener('resize', resize);
		window.visualViewport?.addEventListener('resize', resize);
		return () => {
			if (animationFrame !== undefined) cancelAnimationFrame(animationFrame);
			window.removeEventListener('resize', resize);
			window.visualViewport?.removeEventListener('resize', resize);
		};
	});

	function requestClose(): void {
		if (closing) return;
		closing = true;
		if (animationFrame !== undefined) cancelAnimationFrame(animationFrame);
		const el = contentEl;
		if (!el || !originEl.isConnected) {
			onClose();
			return;
		}
		materialized = false;
		applyRect(el, rectOf(originEl));
		const duration = getComputedStyle(el).getPropertyValue('--dur-3').trim();
		const milliseconds = parseFloat(duration) * (duration.endsWith('ms') ? 1 : 1000);
		closeTimer = setTimeout(onClose, Number.isFinite(milliseconds) ? milliseconds : 0);
	}

	onDestroy(() => {
		if (closeTimer !== undefined) clearTimeout(closeTimer);
		if (animationFrame !== undefined) cancelAnimationFrame(animationFrame);
	});

	function handleOpenChange(next: boolean): void {
		if (!next) requestClose();
	}

	let detail = $state<SessionDetail | null>(null);
	let detailError = $state<string | null>(null);
	let detailLoading = $state(true);
	let retryCount = $state(0);

	$effect(() => {
		const id = node.id;
		void retryCount;
		detail = null;
		detailError = null;
		detailLoading = true;
		let cancelled = false;
		void (async () => {
			try {
				const d = await fetchSession(id, { limit: 1 });
				if (!cancelled) detail = d;
			} catch (err) {
				if (!cancelled) detailError = err instanceof ApiError ? err.message : String(err);
			} finally {
				if (!cancelled) detailLoading = false;
			}
		})();
		return () => {
			cancelled = true;
		};
	});

	const totalTokens = $derived(
		detail?.token_usage
			? detail.token_usage.input_tokens +
					detail.token_usage.output_tokens +
					detail.token_usage.cache_read_tokens +
					detail.token_usage.cache_write_tokens
			: null
	);
</script>

<DialogPrimitive.Root open onOpenChange={handleOpenChange}>
	<Dialog.Portal>
		<Dialog.Overlay />
		<DialogPrimitive.Content
			bind:ref={contentEl}
			data-slot="session-preview-modal"
			class={materialized ? 'spm-content spm-content--materialized' : 'spm-content'}
			data-augmented-ui="tl-clip br-clip border"
			aria-label={`Preview of ${node.title}`}
		>
			<span class="spm-scan" aria-hidden="true"></span>
			<DialogPrimitive.Close data-slot="session-preview-close" class="spm-close" aria-label="Close">
				<X size={14} aria-hidden="true" />
			</DialogPrimitive.Close>
			<div class="spm-body">
				<h3 class="spm-title">{node.title}</h3>
				<div class="spm-meta">
					<span style:color={`var(${graphOutcomeColorVar(node.outcome)})`}
						>{graphOutcomeLabel(node.outcome)}</span
					>
					<span>{Math.round(node.similarity * 100)}% match</span>
				</div>

				{#if detailLoading}
					<p class="spm-hint">Loading…</p>
				{:else if detailError}
					<p class="spm-hint spm-hint--error" role="alert">
						Couldn't load session stats: {detailError}
					</p>
					<Button
						variant="outline"
						size="sm"
						onclick={() => {
							retryCount += 1;
						}}>Retry session stats</Button
					>
				{:else if detail}
					<div class="spm-stats">
						<div class="spm-stat">
							<span class="spm-stat__v"
								>{formatCount(detail.total_requests ?? detail.summary.request_count)}</span
							>
							<span class="spm-stat__l">Requests</span>
						</div>
						<div class="spm-stat">
							<span
								class="spm-stat__v"
								title={totalTokens !== null ? totalTokens.toLocaleString('en-US') : undefined}
								>{formatCompactCount(totalTokens)}</span
							>
							<span class="spm-stat__l">Tokens</span>
						</div>
						<div class="spm-stat">
							<span class="spm-stat__v"
								>{detail.quality?.quality_score != null
									? `${Math.round(detail.quality.quality_score * 100)}%`
									: '—'}</span
							>
							<span class="spm-stat__l">Quality</span>
						</div>
					</div>
					<p class="spm-summary" title={formatClockAbsolute(detail.summary.last_seen)}>
						Last active {formatRelative(detail.summary.last_seen)} · {detail.summary.models.join(
							', '
						) || 'no model recorded'}
					</p>
				{/if}

				<button type="button" class="spm-foot" onclick={() => onOpenSession(node.id)}>
					View full session →
				</button>
			</div>
		</DialogPrimitive.Content>
	</Dialog.Portal>
</DialogPrimitive.Root>

<style>
	:global([data-slot='session-preview-modal']) {
		position: fixed !important;
		margin: 0;
		display: flex;
		flex-direction: column;
		background: var(--surface-3);
		overflow: hidden;
		transition:
			top var(--dur-3, 480ms) var(--ease-snap, cubic-bezier(0.2, 0.9, 0.1, 1)),
			left var(--dur-3, 480ms) var(--ease-snap, cubic-bezier(0.2, 0.9, 0.1, 1)),
			width var(--dur-3, 480ms) var(--ease-snap, cubic-bezier(0.2, 0.9, 0.1, 1)),
			height var(--dur-3, 480ms) var(--ease-snap, cubic-bezier(0.2, 0.9, 0.1, 1));
		--edge: var(--edge-md);
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 2px;
		--aug-border-bg: var(--funk-cyan-border);
	}
	:global([data-slot='session-preview-modal'].spm-content--materialized) {
		animation: spm-glow-once 900ms var(--ease-io, ease) 1;
	}
	@keyframes spm-glow-once {
		0%,
		55% {
			box-shadow: none;
		}
		70% {
			box-shadow: var(--glow-cyan);
		}
		100% {
			box-shadow: none;
		}
	}
	.spm-scan {
		position: absolute;
		left: 0;
		right: 0;
		top: -10%;
		height: 46px;
		background: linear-gradient(180deg, transparent, var(--funk-cyan-border), transparent);
		pointer-events: none;
		z-index: 2;
		opacity: 0;
	}
	:global([data-slot='session-preview-modal'].spm-content--materialized) .spm-scan {
		animation: spm-scan-once 620ms var(--ease-io, ease) 1;
		animation-delay: var(--dur-3, 480ms);
	}
	@keyframes spm-scan-once {
		0% {
			top: -10%;
			opacity: 0;
		}
		12% {
			opacity: 1;
		}
		82% {
			opacity: 1;
		}
		100% {
			top: 100%;
			opacity: 0;
		}
	}

	:global(.spm-close) {
		position: absolute;
		top: var(--space-2, 8px);
		right: var(--space-2, 8px);
		z-index: 3;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		background: none;
		border: 0;
		color: var(--text-3);
		cursor: pointer;
	}
	:global(.spm-close:hover) {
		color: var(--text-strong);
	}
	:global(.spm-close:focus-visible) {
		outline: 2px solid var(--focus);
		outline-offset: 2px;
	}

	.spm-body {
		position: relative;
		z-index: 1;
		flex: 1 1 auto;
		min-height: 0;
		display: flex;
		flex-direction: column;
		padding: var(--space-5, 24px);
		overflow-y: auto;
	}
	.spm-title {
		margin: 0 var(--space-6, 32px) var(--space-2, 8px) 0;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-size: 15px;
		color: var(--text-strong);
		overflow-wrap: anywhere;
	}
	.spm-meta {
		display: flex;
		gap: var(--space-3, 12px);
		font-size: 11px;
		color: var(--text-3);
		text-transform: uppercase;
		letter-spacing: 0.04em;
		margin-bottom: var(--space-4, 16px);
		font-family: var(--font-mono, monospace);
	}
	.spm-hint {
		margin: 0;
		font-size: 12px;
		color: var(--text-3);
		font-style: italic;
	}
	.spm-hint--error {
		color: var(--status-danger-fg);
		font-style: normal;
	}
	.spm-stats {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: var(--space-3, 12px);
		margin-bottom: var(--space-4, 16px);
	}
	.spm-stat {
		border: 1px solid var(--border-subtle);
		padding: var(--space-3, 12px);
		text-align: center;
	}
	.spm-stat__v {
		display: block;
		font-family: var(--font-mono, monospace);
		font-size: 20px;
		color: var(--text-1);
		font-weight: 700;
	}
	.spm-stat__l {
		display: block;
		font-size: 10px;
		color: var(--text-3);
		text-transform: uppercase;
		letter-spacing: 0.04em;
		margin-top: 2px;
	}
	.spm-summary {
		margin: 0;
		font-size: 11px;
		color: var(--text-3);
		font-family: var(--font-mono, monospace);
	}
	.spm-foot {
		margin-top: auto;
		padding-top: var(--space-3, 12px);
		border-top: 1px solid var(--border-subtle);
		background: none;
		border-left: 0;
		border-right: 0;
		border-bottom: 0;
		text-align: left;
		font-size: 12px;
		color: var(--status-info-fg);
		cursor: pointer;
	}
	.spm-foot:hover {
		color: var(--text-strong);
	}
	.spm-foot:focus-visible {
		outline: 2px solid var(--focus);
		outline-offset: 2px;
	}
</style>
