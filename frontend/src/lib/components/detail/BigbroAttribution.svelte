<script lang="ts">
	import { resolve } from '$app/paths';
	import { interventionStatus, verdictKind } from '$lib/steering';
	import type { Authority, SteeringMode, InterventionStatus } from '$lib/steering';
	import { ShipWheel, ArrowRight } from '@lucide/svelte';
	import { getSteeredRequest } from '$lib/api/steering';
	import type { SteeredRequestRow } from '$lib/api/steering';

	interface Props {
		requestId: number;
		sessionId?: string;
	}

	let { requestId, sessionId = '' }: Props = $props();

	let row = $state<SteeredRequestRow | null>(null);
	let loading = $state(false);
	let errorMessage = $state<string | null>(null);
	let copiedAnchor = $state(false);
	let copyTimer: ReturnType<typeof setTimeout> | null = null;

	$effect(() => {
		const id = requestId;
		loading = true;
		errorMessage = null;
		row = null;
		getSteeredRequest(id)
			.then((data) => {
				if (requestId !== id) return;
				row = data;
			})
			.catch((err: unknown) => {
				if (requestId !== id) return;
				errorMessage = err instanceof Error ? err.message : String(err);
			})
			.finally(() => {
				if (requestId === id) loading = false;
			});
	});

	const triggerHref = $derived(
		sessionId && row?.trigger_request_id != null
			? (`sessions/${encodeURIComponent(sessionId)}/requests/${row.trigger_request_id}` as const)
			: null
	);

	const status = $derived.by<InterventionStatus | null>(() => {
		if (!row) return null;
		return interventionStatus({
			sent_at: row.sent_at,
			processed_at: row.processed_at,
			expires_at: row.expires_at,
			reverted_at: row.reverted_at
		});
	});

	const vkind = $derived.by(() => {
		if (!row?.verdict) return null;
		return verdictKind(row.verdict);
	});

	function authorityStyle(authority: Authority | null): string {
		switch (authority) {
			case 'command':
				return 'color: var(--status-warning-fg); background: var(--status-warning-bg); border-color: var(--status-warning-border);';
			case 'redirect':
				return 'color: var(--status-steering-fg); background: var(--status-steering-bg); border-color: var(--status-steering-border);';
			default:
				return 'color: var(--text-2); background: var(--surface-2); border-color: var(--border-subtle);';
		}
	}

	function modeStyle(mode: SteeringMode | null): string {
		switch (mode) {
			case 'drive':
				return 'color: var(--status-steering-fg); background: var(--status-steering-bg); border-color: var(--status-steering-border);';
			default:
				return 'color: var(--text-2); background: var(--surface-2); border-color: var(--border-subtle);';
		}
	}

	function verdictStyle(): string {
		switch (vkind) {
			case 'fired':
				return 'color: var(--status-success-fg); background: var(--status-success-bg); border-color: var(--status-success-border);';
			case 'suppressed':
				return 'color: var(--status-warning-fg); background: var(--status-warning-bg); border-color: var(--status-warning-border);';
			case 'inert':
				return 'color: var(--text-3); background: var(--surface-2); border-color: var(--border-subtle);';
			case 'failed':
				return 'color: var(--status-danger-fg); background: var(--status-danger-bg); border-color: var(--status-danger-border);';
			default:
				return 'color: var(--text-2); background: var(--surface-2); border-color: var(--border-subtle);';
		}
	}

	function formatTimestamp(ts: string | null): string {
		if (!ts) return '—';
		try {
			const d = new Date(ts);
			return d.toISOString().replace('T', ' ').replace(/\.\d+/, '');
		} catch {
			return ts;
		}
	}

	function elapsedSeconds(a: string | null, b: string | null): string | null {
		if (!a || !b) return null;
		const ms = new Date(b).getTime() - new Date(a).getTime();
		if (!Number.isFinite(ms) || ms < 0) return null;
		return `${(ms / 1000).toFixed(1)}s`;
	}

	async function copyAnchor(): Promise<void> {
		if (!row?.anchor_text) return;
		try {
			await navigator.clipboard.writeText(row.anchor_text);
			copiedAnchor = true;
			if (copyTimer !== null) clearTimeout(copyTimer);
			copyTimer = setTimeout(() => {
				copiedAnchor = false;
				copyTimer = null;
			}, 1500);
		} catch {}
	}
</script>

{#if loading}
	<div
		class="bigbro-card bigbro-card--loading"
		data-augmented-ui="tr-clip br-clip border"
		aria-label="Loading steering attribution"
	>
		<span class="bigbro-card__header-label">
			<ShipWheel size={11} aria-hidden="true" />
			BIGBRO ATTRIBUTION
		</span>
		<span class="bigbro-card__loading">Loading…</span>
	</div>
{:else if errorMessage}
	<div
		class="bigbro-card bigbro-card--error"
		data-augmented-ui="tr-clip br-clip border"
		role="alert"
	>
		<span class="bigbro-card__header-label">
			<ShipWheel size={11} aria-hidden="true" />
			BIGBRO ATTRIBUTION
		</span>
		<p class="bigbro-card__error-text">{errorMessage}</p>
	</div>
{:else if row}
	<div
		class="bigbro-card"
		class:bigbro-card--reverted={status === 'reverted'}
		data-augmented-ui="tr-clip br-clip border"
	>
		<div class="bigbro-card__header">
			<span class="bigbro-card__header-label">
				<ShipWheel size={11} aria-hidden="true" />
				BIGBRO ATTRIBUTION
			</span>
			{#if status === 'reverted'}
				<span class="bigbro-card__reverted-chip">REVERTED</span>
			{/if}
		</div>

		<div class="bigbro-card__steerer">
			<span
				class="bigbro-card__steerer-name"
				class:bigbro-card__steerer-name--reverted={status === 'reverted'}
			>
				{row.steerer_name ?? 'unknown steerer'}
			</span>
			{#if row.steerer_model}
				<span
					class="bigbro-card__steerer-model"
					class:bigbro-card__steerer-model--reverted={status === 'reverted'}
				>
					{row.steerer_model}
				</span>
			{/if}
		</div>

		<div class="bigbro-card__badges">
			{#if row.authority}
				<span class="bigbro-badge" style={authorityStyle(row.authority)}>
					{row.authority}
				</span>
			{/if}
			{#if row.mode}
				<span class="bigbro-badge" style={modeStyle(row.mode)}>
					{row.mode}
				</span>
			{/if}
			{#if row.verdict}
				<span class="bigbro-badge" style={verdictStyle()}>
					{row.verdict}
				</span>
			{/if}
			{#if row.confidence !== null}
				<span class="bigbro-card__confidence">
					confidence <span class="bigbro-card__confidence-value">{row.confidence.toFixed(2)}</span>
				</span>
			{/if}
		</div>

		{#if status === 'fired'}
			<div class="bigbro-card__pending">
				<span>Awaiting idle session return</span>
			</div>
			<p class="bigbro-card__pending-meta">
				Sent: {formatTimestamp(row.sent_at)} · Expires: {formatTimestamp(row.expires_at)} · Processed:
				—
			</p>
		{/if}

		{#if status === 'reverted' && (row.revert_reason || row.reverted_at)}
			<div class="bigbro-card__revert-banner">
				{#if row.revert_reason}
					<span class="bigbro-card__revert-label">Revert reason: </span>
					<span class="bigbro-card__revert-text">{row.revert_reason}</span>
				{/if}
				{#if row.reverted_at}
					<p class="bigbro-card__revert-meta">
						Reverted at {formatTimestamp(row.reverted_at)}
					</p>
				{/if}
			</div>
		{/if}

		<div class="bigbro-card__divider"></div>

		{#if row.off_thread_topic}
			<div class="bigbro-card__section">
				<p class="bigbro-card__section-label">Off-thread topic</p>
				<p class="bigbro-card__prose">{row.off_thread_topic}</p>
			</div>
		{/if}

		{#if row.anchor_text}
			<div class="bigbro-card__section">
				<div class="bigbro-card__section-head">
					<p class="bigbro-card__section-label">Anchor at evaluation time</p>
					<button
						type="button"
						class="bigbro-card__copy-btn"
						aria-label="Copy anchor text"
						onclick={copyAnchor}
					>
						{copiedAnchor ? 'copied' : 'copy'}
					</button>
				</div>
				<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
				<pre
					class="bigbro-card__anchor"
					tabindex="0"
					role="region"
					aria-label="Anchor text — scrollable">{row.anchor_text}</pre>
			</div>
		{/if}

		{#if row.payload_text}
			<div class="bigbro-card__section">
				<p class="bigbro-card__section-label">Injected payload</p>
				<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
				<pre
					class="bigbro-card__anchor"
					tabindex="0"
					role="region"
					aria-label="Injected payload — scrollable">{row.payload_text}</pre>
			</div>
		{/if}

		<div class="bigbro-card__divider"></div>

		<div class="bigbro-card__footer">
			{#if row.trigger_request_id !== null}
				<span class="bigbro-card__footer-cell">
					Trigger:
					{#if triggerHref}
						<a href={resolve(triggerHref)} class="bigbro-card__trigger-link">
							#{row.trigger_request_id}
							<ArrowRight class="size-3 inline-block" aria-hidden="true" />
						</a>
					{:else}
						<span class="bigbro-card__trigger-link">#{row.trigger_request_id}</span>
					{/if}
				</span>
			{/if}
			{#if row.sent_at}
				<span class="bigbro-card__footer-cell">
					Sent: {formatTimestamp(row.sent_at)}
				</span>
			{/if}
			{#if row.processed_at}
				{@const elapsed = elapsedSeconds(row.sent_at, row.processed_at)}
				<span class="bigbro-card__footer-cell">
					Processed: {formatTimestamp(row.processed_at)}
					{#if elapsed}
						<span class="bigbro-card__footer-elapsed">({elapsed})</span>
					{/if}
				</span>
			{/if}
		</div>
	</div>
{/if}

<style>
	.bigbro-card {
		--edge: 8px;
		--aug-tr: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--status-steering-fg) 0%,
			var(--status-steering-border) 45%,
			var(--border-default) 80%
		);
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 16px calc(var(--edge) + 4px) 16px 16px;
		border: 1px solid var(--border-default);
		border-left: 3px solid var(--status-steering-border);
		background: var(--surface-card);
		font-family: var(--font-mono, monospace);
	}

	.bigbro-card--reverted {
		background: oklch(from var(--surface-card) l c h / 0.6);
	}

	.bigbro-card--loading,
	.bigbro-card--error {
		display: flex;
		flex-direction: row;
		align-items: center;
		gap: 12px;
	}

	.bigbro-card__header {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.bigbro-card__header-label {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: var(--text-micro, 10px);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--status-steering-fg);
	}

	.bigbro-card__reverted-chip {
		display: inline-flex;
		align-items: center;
		padding: 0 6px;
		border: 1px solid var(--status-danger-border);
		border-radius: 2px;
		background: var(--status-danger-bg);
		font-size: var(--text-micro, 10px);
		font-weight: 600;
		text-transform: uppercase;
		color: var(--status-danger-fg);
	}

	.bigbro-card__loading {
		font-size: 13px;
		color: var(--text-3);
	}

	.bigbro-card__error-text {
		font-size: 13px;
		color: var(--status-danger-fg);
		margin: 0;
	}

	.bigbro-card__steerer {
		display: flex;
		align-items: baseline;
		gap: 8px;
		flex-wrap: wrap;
	}

	.bigbro-card__steerer-name {
		font-size: var(--text-md);
		color: var(--text-1);
	}

	.bigbro-card__steerer-name--reverted {
		text-decoration: line-through;
	}

	.bigbro-card__steerer-model {
		font-size: var(--text-caption, 11px);
		color: var(--text-2);
	}

	.bigbro-card__steerer-model--reverted {
		text-decoration: line-through;
	}

	.bigbro-card__badges {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
	}

	.bigbro-badge {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 1px 6px;
		border: 1px solid transparent;
		border-radius: 2px;
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
	}

	.bigbro-card__confidence {
		font-size: var(--text-caption, 11px);
		color: var(--text-2);
	}

	.bigbro-card__confidence-value {
		color: var(--text-1);
	}

	.bigbro-card__pending {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 12px;
		border: 1px solid var(--status-warning-border);
		border-radius: 2px;
		background: var(--status-warning-bg);
		font-size: var(--text-caption, 11px);
		color: var(--status-warning-fg);
	}

	.bigbro-card__pending-meta {
		margin: 0;
		font-size: var(--text-caption, 11px);
		color: var(--text-2);
	}

	.bigbro-card__revert-banner {
		padding: 8px 12px;
		border: 1px solid var(--status-danger-border);
		border-radius: 2px;
		background: var(--status-danger-bg);
	}

	.bigbro-card__revert-label {
		font-size: var(--text-caption, 11px);
		font-weight: 600;
		color: var(--status-danger-fg);
	}

	.bigbro-card__revert-text {
		font-size: var(--text-caption, 11px);
		color: var(--text-1);
	}

	.bigbro-card__revert-meta {
		margin: 4px 0 0;
		font-size: var(--text-caption, 11px);
		color: var(--text-3);
	}

	.bigbro-card__divider {
		border-top: 1px solid var(--border-subtle);
		margin: 0;
	}

	.bigbro-card__section {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.bigbro-card__section-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}

	.bigbro-card__section-label {
		margin: 0;
		font-size: var(--text-micro, 10px);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
	}

	.bigbro-card__prose {
		margin: 0;
		font-size: 13px;
		color: var(--text-1);
		line-height: 1.6;
		white-space: pre-wrap;
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.bigbro-card__anchor {
		margin: 0;
		max-height: 240px;
		overflow-y: auto;
		padding: 12px;
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		background: var(--surface-2);
		font-family: var(--font-mono, monospace);
		font-size: 13px;
		color: var(--text-1);
		line-height: 1.6;
		white-space: pre-wrap;
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.bigbro-card__anchor:focus-visible {
		outline: 1px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}

	.bigbro-card__copy-btn {
		padding: 2px 6px;
		border-radius: 2px;
		font-size: var(--text-caption, 11px);
		color: var(--text-3);
		background: transparent;
		border: none;
		cursor: pointer;
	}

	.bigbro-card__copy-btn:hover {
		background: var(--surface-2);
		color: var(--text-strong);
	}

	.bigbro-card__footer {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 16px;
	}

	.bigbro-card__footer-cell {
		font-size: var(--text-caption, 11px);
		color: var(--text-2);
	}

	.bigbro-card__footer-elapsed {
		color: var(--text-3);
	}

	.bigbro-card__trigger-link {
		display: inline-flex;
		align-items: center;
		gap: 2px;
		color: var(--text-3);
		text-decoration: underline;
		text-decoration-color: var(--border-default);
		text-underline-offset: 2px;
	}

	.bigbro-card__trigger-link:hover {
		color: var(--text-1);
	}
</style>
