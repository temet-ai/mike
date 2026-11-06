<script module lang="ts">
	import type { DetailMetrics, SessionMatchInfo } from './sessionRowTypes';
	// eslint-disable-next-line no-import-assign -- false positive: re-exporting an imported type from a `<script module>` block trips this rule regardless of syntax (bare, `from`-form, or aliased all reproduce it) since the type-only export is erased at runtime and never actually reassigns the import binding.
	export type { DetailMetrics, SessionMatchInfo };
</script>

<script lang="ts">
	import { ChevronDown, Inbox } from '@lucide/svelte';
	import { resolve } from '$app/paths';
	import { deriveSessionActivity } from '$lib/sessionActivity';
	import { shortSessionId } from '$lib/format';
	import { operatorPromptText } from '$lib/sessionPrompt';
	import { formatClockLabel, formatClockAbsolute, formatCount } from '$lib/time';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import PinToggle from '$lib/components/shared/PinToggle.svelte';
	import CopyIconButton from '$lib/components/shared/CopyIconButton.svelte';
	import SearchMatchRow from './SearchMatchRow.svelte';
	import type { SessionSummary, ProxyEvent } from '$lib/types';

	interface Props {
		session: SessionSummary;
		now: number;
		detail: DetailMetrics | undefined;
		latestWsEventForSession: (sessionId: string) => ProxyEvent | null;
		onHoverIntent: (sessionId: string) => void;
		onHoverCancel: (sessionId: string) => void;
		onTogglePin: (session: SessionSummary) => void;
		matchInfo?: SessionMatchInfo | null;
		matchExpanded?: boolean;
		onToggleMatchExpand?: (sessionId: string) => void;
	}

	let {
		session: s,
		now,
		detail,
		latestWsEventForSession,
		onHoverIntent,
		onHoverCancel,
		onTogglePin,
		matchInfo = null,
		matchExpanded = false,
		onToggleMatchExpand
	}: Props = $props();

	const wsEvent = $derived(latestWsEventForSession(s.id) ?? null);
	const activity = $derived(deriveSessionActivity(s, wsEvent, now));
	const status = $derived(activity.status);
	const isLive = $derived(activity.isLive);
	const hasTitle = $derived(typeof s.title === 'string' && s.title.length > 0);
	const prompt = $derived(operatorPromptText(s.originating_operator_prompt));
	const hasPrompt = $derived(prompt.length > 0);
	const headline = $derived(hasTitle ? (s.title ?? '') : hasPrompt ? prompt : shortSessionId(s.id));
	const headlineIsId = $derived(!hasTitle && !hasPrompt);
	const pendingInboxCount = $derived(s.pending_inbox_count ?? 0);
	const dm = $derived(detail);
	const dmReady = $derived(dm !== undefined && !dm.loading && !dm.error);
	const modelDisplay = $derived(
		dm?.model ?? (s.models.length > 0 ? s.models[s.models.length - 1] : null) ?? '—'
	);
	const costTitle = $derived.by((): string | undefined => {
		const notes: string[] = [];
		if (dm?.partial) {
			notes.push(
				'Aggregated over the most recent requests in this session — the full session has more requests than were fetched.'
			);
		}
		if (dm?.costEstimated) {
			notes.push(
				'Estimate only: some requests used a model we have no price for, so a default price was used for them.'
			);
		}
		return notes.length > 0 ? notes.join(' ') : undefined;
	});
	const stopDisplay = $derived(isLive ? '—' : dmReady ? (dm?.stop ?? '—') : '—');
	const reqWord = $derived(s.request_count === 1 ? 'request' : 'requests');
	const hasSnippets = $derived(
		matchInfo !== null && matchInfo !== undefined
			? matchInfo.conversationSnippets.length > 0 || matchInfo.bodySnippets.length > 0
			: false
	);
</script>

<div
	class="ssn-row-wrap"
	data-motion="hover"
	role="group"
	onpointerenter={() => onHoverIntent(s.id)}
	onpointerleave={() => onHoverCancel(s.id)}
	onfocusin={() => onHoverIntent(s.id)}
>
	<a
		href={resolve(`/sessions/${encodeURIComponent(s.id)}` as const)}
		class="ssn-row"
		aria-label={`${headline}, ${status.label}, ${s.request_count} ${reqWord}, last active ${formatClockLabel(s.last_seen, now)}`}
	>
		<Tooltip text={status.label}>
			{#snippet child({ props })}
				<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
				<span
					{...props}
					class="ssn-dot ssn-dot--{status.kind}"
					class:ssn-dot--live={isLive}
					aria-hidden="true"
					tabindex={-1}
				></span>
			{/snippet}
		</Tooltip>
		<span class="ssn-headline">
			<span class="ssn-headline-text" class:ssn-headline-text--id={headlineIsId}>{headline}</span>
			{#if pendingInboxCount > 0}
				<Tooltip
					text={`${pendingInboxCount} pending ${pendingInboxCount === 1 ? 'message' : 'messages'} waiting for delivery to this session`}
				>
					{#snippet child({ props })}
						<span {...props} class="badge-nx badge-warning ssn-mail-badge">
							<Inbox size={11} aria-hidden="true" />
							{pendingInboxCount}
						</span>
					{/snippet}
				</Tooltip>
			{/if}
		</span>
		<span class="ssn-req"
			>{formatCount(s.request_count)}<span class="ssn-req-unit">{' req'}</span></span
		>
		<span class="ssn-clock ssn-clock--first" title={formatClockAbsolute(s.first_seen)}
			><span class="ssn-clock-label">{'Started '}</span>{formatClockLabel(s.first_seen, now)}</span
		>
		<span class="ssn-clock ssn-clock--last" title={formatClockAbsolute(s.last_seen)}
			><span class="ssn-clock-label">{'Active '}</span>{formatClockLabel(s.last_seen, now)}</span
		>
		<span class="ssn-id-cell">
			<span class="ssn-id-text">{shortSessionId(s.id)}</span>
			<span class="ssn-id-text-full">{s.id}</span>
			<CopyIconButton
				getText={() => s.id}
				label="Copy full session id"
				copiedLabel="Copied"
				size={12}
			/>
			<span class="ssn-row-pin" data-motion="hover" class:ssn-row-pin--pinned={s.pinned === true}>
				<PinToggle pinned={s.pinned === true} onToggle={() => onTogglePin(s)} />
			</span>
		</span>
	</a>
	<div class="ssn-detail">
		<div class="ssn-detail-inner">
			<p class="ssn-detail-prompt">{prompt || s.title || 'No prompt captured for this session.'}</p>
			<div class="ssn-detail-meta">
				<span>MODEL <b class="ssn-detail-model">{modelDisplay}</b></span>
				<span>STATUS <b class="ssn-detail-{status.kind}">{status.label}</b></span>
				<span>IN <b>{dmReady ? formatCount(dm?.inTokens ?? null) : '—'}</b></span>
				<span>OUT <b>{dmReady ? formatCount(dm?.outTokens ?? null) : '—'}</b></span>
				<span
					>CACHE <b class="ssn-detail-cache"
						>{dmReady && dm?.cachePct !== null ? `${dm?.cachePct}%` : '—'}</b
					></span
				>
				<span
					>COST
					<b title={costTitle}>{dmReady ? dm?.costStr : '—'}</b
					>{#if dmReady && dm?.costEstimated && dm.costStr !== '—'}
						<i class="ssn-detail-cost-note">estimate</i>{/if}</span
				>
				<span>STOP <b class="ssn-detail-{status.kind}">{stopDisplay}</b></span>
			</div>
		</div>
	</div>
	{#if matchInfo}
		{@const isExpanded = matchExpanded}
		<div class="ssn-match-row">
			<button
				type="button"
				class="ssn-match-toggle"
				disabled={!hasSnippets}
				aria-expanded={hasSnippets ? isExpanded : undefined}
				aria-controls={hasSnippets ? `ssn-match-panel-${s.id}` : undefined}
				onclick={() => hasSnippets && onToggleMatchExpand?.(s.id)}
			>
				<span class="ssn-match-label">matched in</span>
				{#if matchInfo.titleHit}<span class="ssn-match-chip">title</span>{/if}
				{#if matchInfo.conversationCount > 0}
					<span class="ssn-match-chip">{matchInfo.conversationCount} conversation</span>
				{/if}
				{#if matchInfo.bodyCount > 0}
					<span class="ssn-match-chip">{matchInfo.bodyCount} raw body</span>
				{/if}
				{#if hasSnippets}
					<ChevronDown
						class="ssn-match-caret {isExpanded ? 'ssn-match-caret--open' : ''}"
						data-motion="disclose-marker"
						size={11}
						aria-hidden="true"
					/>
				{/if}
			</button>
		</div>
		{#if hasSnippets && isExpanded}
			<ul
				class="ssn-match-panel"
				id="ssn-match-panel-{s.id}"
				role="list"
				aria-label="Matches in this session"
			>
				{#each matchInfo.conversationSnippets as m, i (`c-${m.request_id}-${i}`)}
					<SearchMatchRow sessionId={s.id} match={m} />
				{/each}
				{#each matchInfo.bodySnippets as m, i (`b-${m.request_id}-${i}`)}
					<SearchMatchRow sessionId={s.id} match={m} />
				{/each}
			</ul>
			{#if matchInfo.conversationTruncated}
				<p class="ssn-match-note">
					Showing {matchInfo.conversationSnippets.length} of {matchInfo.conversationCount} conversation
					matches — refine the search to narrow.
				</p>
			{/if}
		{/if}
	{/if}
</div>

<style>
	.ssn-row-wrap {
		--rev: 0;
		position: relative;
		border-top: 1px solid var(--border-subtle);
	}
	.ssn-row-wrap:first-child {
		border-top: none;
	}
	.ssn-row-wrap:hover,
	.ssn-row-wrap:focus-within {
		--rev: 1;
		background: var(--surface-2);
	}

	.ssn-row-wrap::before {
		content: '';
		position: absolute;
		inset-block: 0;
		left: 0;
		width: 2px;
		background: linear-gradient(180deg, var(--data-teal-fg), var(--data-teal-border));
		opacity: var(--rev);
		transform: scaleY(var(--rev));
		transform-origin: center;
		pointer-events: none;
	}
	.ssn-row {
		display: grid;
		grid-template-columns: 14px minmax(0, 1fr) 70px 110px 110px 190px;
		gap: 14px;
		align-items: center;
		padding: 4px 16px;
		text-decoration: none;
		color: inherit;
		cursor: pointer;
	}
	.ssn-row:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}

	.ssn-dot {
		width: 7px;
		height: 7px;
		background: var(--text-faint);
		justify-self: center;
		flex-shrink: 0;
	}
	.ssn-dot--complete {
		background: var(--status-success-fg);
	}
	.ssn-dot--error {
		background: var(--status-danger-fg);
	}
	.ssn-dot--streaming,
	.ssn-dot--pending {
		background: var(--status-warning-fg);
	}
	.ssn-dot--live {
		box-shadow: 0 0 0 3px oklch(from var(--status-warning-fg) l c h / 0.13);
	}

	.ssn-headline {
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
	}
	.ssn-headline-text {
		min-width: 0;
		font-size: var(--text-sm);
		font-weight: 500;
		color: var(--text-strong);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.ssn-headline-text--id {
		font-family: var(--font-mono);
		font-weight: 400;
		color: var(--text-3);
	}
	.ssn-mail-badge {
		flex-shrink: 0;
		font-variant-numeric: tabular-nums;
	}

	.ssn-req {
		text-align: right;
		font-size: var(--text-sm);
		font-weight: 600;
		color: var(--text-strong);
		font-variant-numeric: tabular-nums;
	}
	.ssn-req-unit {
		display: none;
	}
	.ssn-clock {
		font-size: var(--text-label);
		color: var(--text-2);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	.ssn-clock-label {
		display: none;
	}

	.ssn-id-cell {
		display: flex;
		align-items: center;
		gap: 6px;
		min-width: 0;
		justify-content: flex-end;
	}
	.ssn-id-text {
		font-family: var(--font-mono);
		font-size: var(--text-label);
		letter-spacing: 0.02em;
		color: var(--text-faint);
		white-space: nowrap;
	}
	.ssn-id-text-full {
		display: none;
	}
	.ssn-row-pin {
		flex-shrink: 0;
		opacity: 0;
	}
	.ssn-row-wrap:hover .ssn-row-pin,
	.ssn-row-wrap:focus-within .ssn-row-pin,
	.ssn-row-pin--pinned {
		opacity: 1;
	}

	@container session-table (max-width: 799.98px) {
		.ssn-row {
			grid-template-columns: 14px minmax(0, 1fr) max-content;
			grid-template-areas:
				'dot head req'
				'dot id id'
				'dot start last';
			column-gap: var(--space-3, 12px);
			row-gap: var(--space-1, 4px);
			align-items: start;
			padding: var(--space-2, 8px) var(--space-4, 16px);
		}
		.ssn-dot {
			grid-area: dot;
			margin-top: 5px;
		}
		.ssn-headline {
			grid-area: head;
		}
		.ssn-req {
			grid-area: req;
		}
		.ssn-id-cell {
			grid-area: id;
			justify-content: flex-start;
			flex-wrap: wrap;
		}
		.ssn-clock--first {
			grid-area: start;
		}
		.ssn-clock--last {
			grid-area: last;
			text-align: right;
		}

		.ssn-headline-text {
			white-space: normal;
			overflow: visible;
			text-overflow: clip;
			overflow-wrap: anywhere;
		}
		.ssn-id-text {
			display: none;
		}
		.ssn-id-text-full {
			display: inline;
			white-space: normal;
			overflow-wrap: anywhere;
			font-family: var(--font-mono);
			font-size: var(--text-label);
			letter-spacing: 0.02em;
			color: var(--text-faint);
		}

		.ssn-clock-label {
			display: inline;
		}
		.ssn-req-unit {
			display: inline;
			font-size: var(--text-caption);
			color: var(--text-faint);
		}
	}

	.ssn-detail {
		overflow: hidden;
		max-height: calc(var(--rev) * 88px);
		opacity: var(--rev);
		transition:
			max-height var(--dur-2) var(--ease-io),
			opacity var(--dur-2) var(--ease-io);
	}
	.ssn-detail-inner {
		padding: 0 16px 11px 37px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.ssn-detail-prompt {
		margin: 0;
		font-size: var(--text-label);
		line-height: 1.5;
		color: var(--text-2);
		overflow-wrap: anywhere;
	}
	.ssn-detail-meta {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px 18px;
		font-size: var(--text-micro);
		letter-spacing: 0.06em;
		color: var(--text-faint);
	}
	.ssn-detail-meta b {
		font-size: var(--text-caption);
		letter-spacing: 0.02em;
		font-weight: 500;
		font-variant-numeric: tabular-nums;
		color: var(--text-1);
	}
	.ssn-detail-model {
		color: var(--status-info-fg) !important;
	}
	.ssn-detail-cost-note {
		font-style: normal;
		text-transform: lowercase;
		color: var(--text-faint);
	}
	.ssn-detail-cache {
		color: var(--data-teal-fg) !important;
	}
	.ssn-detail-complete {
		color: var(--status-success-fg) !important;
	}
	.ssn-detail-streaming,
	.ssn-detail-pending {
		color: var(--status-warning-fg) !important;
	}
	.ssn-detail-error {
		color: var(--status-danger-fg) !important;
	}
	.ssn-detail-unknown {
		color: var(--text-1) !important;
	}

	.ssn-match-row {
		border-top: 1px solid var(--border-subtle);
	}
	.ssn-match-toggle {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		border: 0;
		background: transparent;
		cursor: pointer;
		padding: 6px 16px 6px 37px;
		font-family: var(--font-mono);
		text-align: left;
	}
	.ssn-match-toggle:disabled {
		cursor: default;
	}
	.ssn-match-toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}
	.ssn-match-label {
		font-size: var(--text-micro);
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--text-faint);
		flex-shrink: 0;
	}
	.ssn-match-chip {
		flex-shrink: 0;
		padding: 1px 7px;
		border-radius: 2px;
		border: 1px solid var(--border-subtle);
		background: var(--surface-2);
		font-size: var(--text-caption);
		color: var(--text-2);
		white-space: nowrap;
	}
	.ssn-match-toggle :global(.ssn-match-caret) {
		flex-shrink: 0;
		margin-left: auto;
		color: var(--text-faint);
	}
	.ssn-match-toggle :global(.ssn-match-caret--open) {
		transform: rotate(180deg);
	}

	.ssn-match-panel {
		list-style: none;
		margin: 0;
		padding: 0 16px 12px 37px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.ssn-match-note {
		margin: 0;
		padding: 0 16px 12px 37px;
		font-size: var(--text-caption);
		font-style: italic;
		color: var(--text-faint);
	}
</style>
