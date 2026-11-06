<script lang="ts">
	import { ArrowUpRight } from '@lucide/svelte';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';

	export type VerdictCode =
		| 'fired'
		| 'suppressed_debounce'
		| 'suppressed_arbitration'
		| 'no_drift'
		| 'gate_miss'
		| 'failed_closed';

	export type AuthorityCode = 'supply_context' | 'redirect' | 'command';
	export type ModeCode = 'augment' | 'drive';

	export interface SteererEvent {
		createdAt: string;
		verdict: VerdictCode;
		confidence: number | null;
		gateReason: string | null;
		payloadText: string | null;
		triggerRequestId: number | null;
		sentRequestId: number | null;
	}

	export interface SteererData {
		steererDisplayName: string | null;
		steererName: string | null;
		description: string | null;
		authority: AuthorityCode | null;
		mode: ModeCode | null;
		events: SteererEvent[];
	}

	interface Props {
		steerer: SteererData;
		onJumpTo?: (requestId: number) => void;
	}

	let { steerer, onJumpTo }: Props = $props();

	const VERDICT_LABELS: Record<VerdictCode, string> = {
		fired: 'Fired',
		suppressed_debounce: 'Held back (debounce)',
		suppressed_arbitration: 'Held back (arbitration)',
		no_drift: 'No drift detected',
		gate_miss: 'Gate not met',
		failed_closed: 'Failed safe'
	};

	const VERDICT_TOOLTIPS: Record<VerdictCode, string> = {
		fired: 'The steerer judged drift above threshold and injected a nudge on the next turn.',
		suppressed_debounce:
			'Held back (debounce): the steerer judged drift and would have fired, but was suppressed ' +
			'because it already fired within the last debounce_turns turns ' +
			'(a re-fire cooldown so it does not nag every turn).',
		suppressed_arbitration:
			'Held back (arbitration): another steerer with higher priority fired first this turn.',
		no_drift: 'No drift detected: the steerer evaluated but confidence was below threshold.',
		gate_miss:
			'Gate not met: the gate condition was not triggered — evaluator did not run this turn.',
		failed_closed: 'Failed safe: the steerer evaluator errored and did not inject.'
	};

	const AUTHORITY_LABELS: Record<AuthorityCode, string> = {
		supply_context: 'Supply context',
		redirect: 'Redirect',
		command: 'Command'
	};

	const AUTHORITY_TOOLTIPS: Record<AuthorityCode, string> = {
		supply_context: 'supply_context — supply missing context/facts to the session.',
		redirect: 'redirect — call out the drift and point attention back to the thread.',
		command: 'command — issue a direct instruction.'
	};

	const MODE_LABELS: Record<ModeCode, string> = {
		augment: 'Augment',
		drive: 'Drive'
	};

	const MODE_TOOLTIPS: Record<ModeCode, string> = {
		augment: 'augment — adds a nudge alongside the conversation.',
		drive: 'drive — takes a stronger, driving steering role.'
	};

	const CONFIDENCE_TOOLTIP =
		"confidence (0–1): the steerer's LLM-judge certainty that the session has drifted from its thread anchor. " +
		"The intervention only fires when confidence ≥ the steerer's confidence_threshold (typically 0.7–0.95).";

	function formatTimestamp(iso: string): string {
		try {
			return new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
		} catch {
			return iso;
		}
	}

	function verdictStyle(v: VerdictCode): string {
		switch (v) {
			case 'fired':
				return 'color: var(--status-success-fg); background: linear-gradient(0deg, var(--status-success-bg), var(--status-success-bg)), var(--surface-card); --aug-border-bg: var(--status-success-border);';
			case 'suppressed_debounce':
			case 'suppressed_arbitration':
				return 'color: var(--text-2); background: linear-gradient(0deg, var(--surface-2), var(--surface-2)), var(--surface-card); --aug-border-bg: var(--border-subtle);';
			case 'no_drift':
				return 'color: var(--text-3); background: linear-gradient(0deg, var(--surface-2), var(--surface-2)), var(--surface-card); --aug-border-bg: var(--border-subtle);';
			case 'gate_miss':
				return 'color: var(--text-3); background: var(--surface-card); --aug-border-bg: transparent;';
			case 'failed_closed':
				return 'color: var(--status-danger-fg); background: linear-gradient(0deg, var(--status-danger-bg), var(--status-danger-bg)), var(--surface-card); --aug-border-bg: var(--status-danger-border);';
		}
	}

	let expanded = $state(false);

	const hasFired = $derived(steerer.events.some((e) => e.verdict === 'fired'));
</script>

<div
	class="ss-card"
	data-augmented-ui="tl-clip br-clip border"
	style="--aug-border-bg: linear-gradient(135deg, {hasFired
		? 'var(--status-steering-fg)'
		: 'var(--border-subtle)'} 0%, var(--border-default) 45%);"
>
	<div class="ss-head">
		<span class="ss-name">
			{steerer.steererDisplayName ?? steerer.steererName ?? 'unknown'}
		</span>

		{#if steerer.authority}
			<span class="chip-backing">
				<abbr
					class="ss-tag"
					data-augmented-ui="tl-clip br-clip border"
					title={AUTHORITY_TOOLTIPS[steerer.authority]}
				>
					{AUTHORITY_LABELS[steerer.authority]}
				</abbr>
			</span>
		{/if}
		{#if steerer.mode}
			<span class="chip-backing">
				<abbr
					class="ss-tag"
					data-augmented-ui="tl-clip br-clip border"
					title={MODE_TOOLTIPS[steerer.mode]}
				>
					{MODE_LABELS[steerer.mode]}
				</abbr>
			</span>
		{/if}

		<span class="ss-spacer" aria-hidden="true"></span>
		<span class="ss-count">
			{steerer.events.length}
			{steerer.events.length === 1 ? 'event' : 'events'}
		</span>
	</div>

	{#if steerer.description}
		<p class="ss-desc">{steerer.description}</p>
	{:else}
		<p class="ss-desc ss-desc--missing">
			<em>No description — requires SteererDto join by steerer_name.</em>
		</p>
	{/if}

	<div class="ss-expand-row">
		<button
			type="button"
			class="ss-expand-btn"
			aria-expanded={expanded}
			onclick={() => (expanded = !expanded)}
		>
			<DisclosureMarker open={expanded} size={11} />
			{expanded ? 'Hide interventions' : `Show interventions (${steerer.events.length})`}
		</button>
	</div>

	{#if expanded}
		<div class="ss-timeline" role="list" aria-label="Intervention history">
			{#each steerer.events as event (event.createdAt)}
				{@const jumpTarget = event.sentRequestId ?? event.triggerRequestId}
				<div class="ss-event ss-event--{event.verdict}" role="listitem">
					<span class="ss-event-time">{formatTimestamp(event.createdAt)}</span>

					<span class="chip-backing">
						<abbr
							class="ss-event-verdict"
							data-augmented-ui="tl-clip br-clip border"
							style={verdictStyle(event.verdict)}
							title={VERDICT_TOOLTIPS[event.verdict]}
						>
							{VERDICT_LABELS[event.verdict]}
						</abbr>
					</span>

					{#if event.confidence !== null}
						<abbr class="ss-conf" title={CONFIDENCE_TOOLTIP}>
							confidence {event.confidence.toFixed(2)}
						</abbr>
					{/if}

					{#if event.gateReason}
						<span class="ss-event-reason">{event.gateReason}</span>
					{/if}

					<span class="ss-event-spacer" aria-hidden="true"></span>

					{#if jumpTarget !== null}
						<span class="chip-backing">
							<button
								type="button"
								class="ss-event-jump"
								data-augmented-ui="tr-clip bl-clip border"
								onclick={() => onJumpTo?.(jumpTarget)}
								aria-label="Jump to request #{jumpTarget}"
							>
								<ArrowUpRight size={10} aria-hidden="true" /> #{jumpTarget}
							</button>
						</span>
					{/if}

					{#if event.payloadText}
						<div class="ss-payload-wrap">
							<p class="ss-payload-label">What it did:</p>
							<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
							<pre
								class="ss-payload"
								data-augmented-ui="tr-clip br-clip border"
								tabindex="0">{event.payloadText}</pre>
						</div>
					{/if}
				</div>
			{/each}
		</div>
	{/if}
</div>

<style>
	.ss-card {
		--edge: 8px;
		--edge-br: 12px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge-br);
		--aug-border-all: 1px;
		display: flex;
		flex-direction: column;
		gap: 6px;
		min-width: 0;
		background: var(--surface-card);
		box-shadow:
			inset 0 1px 0 var(--surface-shine),
			var(--shadow-sm);
		padding: calc(var(--edge) + 4px) calc(var(--edge-br) + 4px) calc(var(--edge-br) + 4px)
			calc(var(--edge) + 4px);
	}

	.chip-backing {
		display: inline-flex;
		background: var(--surface-card);
	}

	.ss-head {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px;
	}

	.ss-name {
		font-family: var(--font-mono, monospace);
		font-size: 13px;
		font-weight: 600;
		color: var(--text-strong);
	}

	.ss-tag {
		--aug-tl: 6px;
		--aug-br: 6px;
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		padding: 1px 6px;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-2);
		background: linear-gradient(0deg, var(--surface-2), var(--surface-2)), var(--surface-card);
		text-decoration: none;
		border-bottom: 1px dotted var(--border-default);
		cursor: help;
	}

	.ss-spacer {
		flex: 1 1 auto;
	}

	.ss-count {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
	}

	.ss-desc {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		line-height: 1.5;
		color: var(--text-1);
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.ss-desc--missing {
		color: var(--text-3);
	}

	.ss-expand-row {
		display: flex;
	}

	.ss-expand-btn {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 2px 0;
		border: none;
		background: transparent;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-2);
		cursor: pointer;
		transition: color var(--dur-1) var(--ease-snap);
	}

	.ss-expand-btn:hover {
		color: var(--text-strong);
	}

	.ss-expand-btn:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.ss-timeline {
		display: flex;
		flex-direction: column;
		gap: 0;
		border-top: 1px solid var(--border-subtle);
		padding-top: 6px;
		margin-top: 2px;
	}

	.ss-event {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 6px;
		padding: 5px 0;
		border-bottom: 1px solid oklch(from var(--border-subtle) l c h / 0.4);
	}

	.ss-event:last-child {
		border-bottom: none;
	}

	.ss-event--fired {
		box-shadow: inset 3px 0 0 0 var(--status-steering-fg);
		padding-left: 6px;
	}

	.ss-event-time {
		flex: 0 0 44px;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-3);
		font-variant-numeric: tabular-nums;
		align-self: center;
	}

	.ss-event-verdict {
		--aug-tl: 6px;
		--aug-br: 6px;
		--aug-border-all: 1px;
		padding: 1px 5px;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 600;
		white-space: nowrap;
		flex-shrink: 0;
		text-decoration: none;
		border-bottom: 1px dotted var(--border-default);
		cursor: help;
		align-self: center;
	}

	.ss-conf {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-3);
		flex-shrink: 0;
		text-decoration: none;
		border-bottom: 1px dotted var(--border-default);
		cursor: help;
		align-self: center;
	}

	.ss-event-reason {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-1);
		overflow-wrap: break-word;
		word-break: break-word;
		flex: 1 1 200px;
	}

	.ss-event-spacer {
		flex: 1 1 auto;
	}

	.ss-event-jump {
		--aug-tr: 6px;
		--aug-bl: 6px;
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		display: inline-flex;
		align-items: center;
		gap: 2px;
		padding: 1px 5px;
		background: var(--surface-card);
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--status-info-fg);
		font-weight: 600;
		cursor: pointer;
		flex-shrink: 0;
		align-self: center;
		transition:
			color var(--dur-1) var(--ease-snap),
			background var(--dur-1) var(--ease-snap);
	}

	.ss-event-jump:hover {
		background:
			linear-gradient(0deg, var(--interactive-hover-surface), var(--interactive-hover-surface)),
			var(--surface-card);
		--aug-border-bg: var(--border-default);
	}

	.ss-event-jump:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 1px;
	}

	.ss-payload-wrap {
		flex: 0 0 100%;
		display: flex;
		flex-direction: column;
		gap: 3px;
		padding-top: 4px;
	}

	.ss-payload-label {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--status-steering-fg);
	}

	.ss-payload {
		--edge: 4px;
		--aug-tr: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--status-steering-border, var(--border-subtle));
		margin: 0;
		max-height: 200px;
		overflow-y: auto;
		padding: 8px calc(var(--edge) + 4px) 8px 10px;
		background: var(--surface-2);
		border-left: 3px solid var(--status-steering-fg);
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-1);
		line-height: 1.5;
		white-space: pre-wrap;
		overflow-wrap: break-word;
		word-break: break-word;
	}
</style>
