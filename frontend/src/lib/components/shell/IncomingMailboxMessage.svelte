<script lang="ts">
	import { getContext } from 'svelte';
	import { resolve } from '$app/paths';
	import { Mail, Clock, ExternalLink } from '@lucide/svelte';
	import type { MailboxMessage } from '$lib/api/mailbox';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';
	import { shortSessionId } from '$lib/format';
	import { formatRelative } from '$lib/time';
	import { splitFirstLine, highlightSegments } from './timelineTurnView';
	import { cn } from '$lib/utils';

	interface Props {
		message: MailboxMessage;
		variant: 'delivered' | 'queued';
		sessionId: string;
		now: number;
		turnIndex: number | null;
		query: string;
	}

	let { message, variant, sessionId, now, turnIndex, query }: Props = $props();

	const collapseStore = getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY);
	let localOpen = $state(false);
	const collapseKey = $derived(`timeline-mbx:${sessionId}:${message.message_uuid}`);
	const open = $derived(collapseStore ? collapseStore.isExpanded(collapseKey, false) : localOpen);

	function toggle(): void {
		if (collapseStore) collapseStore.toggle(collapseKey);
		else localOpen = !localOpen;
	}

	const bodyPreview = $derived(splitFirstLine(message.body));
	const senderShort = $derived(
		message.from_session_uuid ? shortSessionId(message.from_session_uuid) : '—'
	);
	const queuedLabel = $derived(message.status === 'claimed' ? 'claiming' : 'pending');
</script>

<div class="im-root" data-slot="incoming-mailbox-message">
	{#if variant === 'delivered'}
		<div class="im-slot" aria-hidden="true"></div>
	{/if}
	<div
		class={cn(
			'im-card',
			variant === 'queued' ? 'im-card--queued' : 'im-card--delivered im-anim-card'
		)}
		data-augmented-ui="tl-clip br-clip border"
	>
		<div class="im-row-wrap">
			<button
				type="button"
				class="im-toggle"
				aria-expanded={open}
				aria-label={`${open ? 'Collapse' : 'Expand'} ${variant === 'delivered' ? 'incoming' : 'queued'} message from ${senderShort}`}
				onclick={toggle}
			>
				<div class="im-head">
					<span
						class={cn('im-icon-box', variant === 'queued' ? 'im-icon-box--queued' : 'im-anim-env')}
					>
						{#if variant === 'delivered'}
							<Mail size={12} aria-hidden="true" />
						{:else}
							<Clock size={12} aria-hidden="true" />
						{/if}
					</span>
					<span class="im-label" class:im-label--queued={variant === 'queued'}>
						{variant === 'delivered' ? 'Incoming message' : 'Queued message'}
					</span>
					<span class="im-meta">
						<span title={message.from_session_uuid ?? ''}>from {senderShort}</span>
						{#if variant === 'delivered'}
							<span class="im-meta__sep" aria-hidden="true">·</span>
							<span
								>delivered into turn {turnIndex !== null
									? String(turnIndex).padStart(2, '0')
									: '—'}</span
							>
							{#if message.sent_request_id !== null}
								<span class="im-meta__sep" aria-hidden="true">→</span>
								<span class="im-meta__req">#{message.sent_request_id}</span>
							{/if}
							<span class="im-meta__sep" aria-hidden="true">·</span>
							<span title={message.sent_at ?? ''}>{formatRelative(message.sent_at, now)}</span>
						{:else}
							<span class="im-meta__sep" aria-hidden="true">·</span>
							<span title={message.queued_at}
								>{queuedLabel} — queued {formatRelative(message.queued_at, now)}</span
							>
						{/if}
					</span>
				</div>

				{#if variant === 'queued'}
					<div class="im-queued-line" aria-hidden="true">
						<span class="im-dot">·</span><span class="im-dot">·</span><span class="im-dot">·</span>
						<span class="im-queued-text">waiting for next turn</span>
					</div>
				{/if}

				<div class={cn('im-body-wrap', variant === 'delivered' && 'im-anim-body')}>
					{#if open}
						<p class="im-body-full">
							{#each highlightSegments(message.body, query) as seg, i (i)}
								{#if seg.mark}<mark class="im-mark">{seg.text}</mark>{:else}{seg.text}{/if}
							{/each}
						</p>
					{:else}
						<p class="im-body-preview">{bodyPreview.first}</p>
						<p class="im-hint">click message to expand</p>
					{/if}
				</div>
			</button>
			{#if message.from_session_uuid}
				{@const fromUuid = message.from_session_uuid}
				<a
					class="im-sender-link"
					href={resolve(`/sessions/${encodeURIComponent(fromUuid)}`)}
					title="Open sender session"
					aria-label={`Open sender session ${senderShort}`}
					onclick={(e) => e.stopPropagation()}
				>
					<ExternalLink size={11} aria-hidden="true" />
				</a>
			{/if}
		</div>
	</div>
</div>

<style>
	.im-root {
		display: flex;
		flex-direction: column;
	}

	.im-slot {
		height: 3px;
		border-radius: 2px;
		background: var(--status-info-border);
		opacity: 0.6;
		margin: 0 16px;
	}

	.im-card {
		--edge: 8px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		background: var(--surface-card);
	}

	.im-card--delivered {
		--aug-border-bg: var(--status-info-border);
		background: var(--status-info-bg);
	}

	.im-card--queued {
		--aug-border-bg: var(--status-warning-border);
		background: var(--status-warning-bg);
	}

	.im-row-wrap {
		display: flex;
		align-items: flex-start;
		min-width: 0;
	}

	.im-toggle {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 10px 12px;
		border: none;
		background: transparent;
		text-align: left;
		font: inherit;
		cursor: pointer;
		color: inherit;
	}

	.im-toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}

	.im-head {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
	}

	.im-icon-box {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 20px;
		height: 20px;
		flex-shrink: 0;
		border-radius: 2px;
		background: var(--status-info-bg);
		border: 1px solid var(--status-info-border);
		color: var(--status-info-fg);
	}

	.im-icon-box--queued {
		background: var(--status-warning-bg);
		border-color: var(--status-warning-border);
		color: var(--status-warning-fg);
	}

	.im-label {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 700;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--status-info-fg);
	}

	.im-label--queued {
		color: var(--status-warning-fg);
	}

	.im-meta {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		flex-wrap: wrap;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-3);
	}

	.im-meta__sep {
		color: var(--text-faint);
	}

	.im-meta__req {
		color: var(--status-success-fg);
	}

	.im-queued-line {
		display: flex;
		align-items: center;
		gap: 2px;
		padding-left: 28px;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--status-warning-fg);
	}

	.im-queued-text {
		margin-left: 6px;
		color: var(--text-3);
	}

	.im-dot {
		opacity: 0.3;
	}

	.im-body-wrap {
		padding-left: 28px;
	}

	.im-body-preview {
		margin: 0;
		overflow-wrap: anywhere;
		font-size: 12px;
		line-height: 1.5;
		color: var(--text-1);
	}

	.im-hint {
		margin: 2px 0 0;
		font-size: 10px;
		color: var(--text-faint);
	}

	.im-body-full {
		margin: 0;
		font-size: 12px;
		line-height: 1.5;
		color: var(--text-1);
		white-space: pre-wrap;
		word-break: break-word;
	}

	.im-mark {
		background: var(--interactive-active-bg);
		color: var(--interactive-active-fg);
	}

	.im-sender-link {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		margin: 10px 10px 0 0;
		flex-shrink: 0;
		border-radius: 2px;
		color: var(--text-3);
	}

	.im-sender-link:hover {
		color: var(--interactive-active-fg);
		background: var(--surface-2);
	}

	.im-sender-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -1px;
	}

	.im-anim-card {
		animation: im-card-drop var(--dur-3) var(--ease-snap) both;
	}
	.im-anim-env {
		animation: im-env-fly var(--dur-3) var(--ease-snap) 300ms both;
	}
	.im-anim-body {
		animation: im-body-reveal var(--dur-2) ease-out 480ms both;
	}

	@keyframes im-card-drop {
		0% {
			clip-path: inset(0 0 100% 0);
			transform: translateY(-8px);
		}
		100% {
			clip-path: inset(-8px 0 -8px 0);
			transform: translateY(0);
		}
	}

	@keyframes im-env-fly {
		0% {
			transform: translate(-10px, -6px) rotate(-10deg);
			opacity: 0;
		}
		55% {
			transform: translate(1px, 1px) rotate(3deg);
			opacity: 1;
		}
		100% {
			transform: translate(0, 0) rotate(0deg);
			opacity: 1;
		}
	}

	@keyframes im-body-reveal {
		0% {
			opacity: 0;
			transform: translateY(4px);
		}
		100% {
			opacity: 1;
			transform: translateY(0);
		}
	}
</style>
