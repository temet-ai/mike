<script lang="ts">
	import { ArrowUpRight } from '@lucide/svelte';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';

	export type DispatchOutcome = 'completed' | 'abandoned' | 'error' | null;

	export interface DispatchEvent {
		requestId: number;
		createdAt: string;
		outcome: DispatchOutcome;
		description: string | null;
	}

	export interface AgentData {
		subagentType: string;
		model: string | null;
		description: string;
		events: DispatchEvent[];
	}

	interface Props {
		agent: AgentData;
		agentColor: string;
		onJumpTo?: (requestId: number) => void;
	}

	let { agent, agentColor, onJumpTo }: Props = $props();

	const OUTCOME_LABELS: Record<Exclude<DispatchOutcome, null>, string> = {
		completed: 'Completed',
		abandoned: 'Abandoned',
		error: 'Errored'
	};

	const OUTCOME_TOOLTIPS: Record<Exclude<DispatchOutcome, null> | 'inflight', string> = {
		completed: 'The agent sub-task finished and reported back.',
		abandoned: 'The agent sub-task was cancelled or did not report back.',
		error: 'The agent sub-task terminated with an error.',
		inflight: 'The agent sub-task is still running — no outcome yet.'
	};

	const MODEL_TOOLTIPS: Record<string, string> = {
		'claude-sonnet-4-6':
			'claude-sonnet-4-6 — Anthropic Sonnet 4.6. Balanced performance and speed.',
		'claude-haiku-4-5-20251001':
			'claude-haiku-4-5-20251001 — Anthropic Haiku 4.5. Fast and cost-effective.',
		'claude-opus-4-8': 'claude-opus-4-8 — Anthropic Opus 4.8. Highest capability.'
	};

	function modelTooltip(model: string): string {
		return MODEL_TOOLTIPS[model] ?? model;
	}

	function outcomeStyle(outcome: DispatchOutcome): string {
		switch (outcome) {
			case 'completed':
				return 'color: var(--status-success-fg); background: linear-gradient(0deg, var(--status-success-bg), var(--status-success-bg)), var(--surface-card); --aug-border-bg: var(--status-success-border);';
			case 'abandoned':
				return 'color: var(--text-2); background: linear-gradient(0deg, var(--surface-2), var(--surface-2)), var(--surface-card); --aug-border-bg: var(--border-subtle);';
			case 'error':
				return 'color: var(--status-danger-fg); background: linear-gradient(0deg, var(--status-danger-bg), var(--status-danger-bg)), var(--surface-card); --aug-border-bg: var(--status-danger-border);';
			case null:
				return 'color: var(--status-info-fg); background: linear-gradient(0deg, var(--surface-2), var(--surface-2)), var(--surface-card); --aug-border-bg: var(--border-subtle);';
		}
	}

	function formatTimestamp(iso: string): string {
		try {
			return new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
		} catch {
			return iso;
		}
	}

	let expanded = $state(false);
</script>

<div
	class="sa-card"
	data-augmented-ui="tl-clip br-clip border"
	style="--aug-border-bg: linear-gradient(135deg, {agentColor} 0%, var(--border-default) 45%);"
>
	<div class="sa-head">
		<span class="sa-type" style="color: {agentColor};">{agent.subagentType}</span>

		{#if agent.model}
			<span class="chip-backing">
				<abbr
					class="sa-model"
					data-augmented-ui="tl-clip br-clip border"
					title={modelTooltip(agent.model)}
				>
					{agent.model}
				</abbr>
			</span>
		{/if}

		<span class="sa-spacer" aria-hidden="true"></span>
		<span class="sa-count">
			{agent.events.length}
			{agent.events.length === 1 ? 'dispatch' : 'dispatches'}
		</span>
	</div>

	<p class="sa-desc">{agent.description}</p>

	<div class="sa-expand-row">
		<button
			type="button"
			class="sa-expand-btn"
			aria-expanded={expanded}
			onclick={() => (expanded = !expanded)}
		>
			<DisclosureMarker open={expanded} size={11} />
			{expanded ? 'Hide dispatches' : `Show dispatches (${agent.events.length})`}
		</button>
	</div>

	{#if expanded}
		<div class="sa-timeline" role="list" aria-label="Dispatch history">
			{#each agent.events as event (event.requestId)}
				{@const outcomeKey = event.outcome}
				<div
					class="sa-event {outcomeKey ? `sa-event--${outcomeKey}` : 'sa-event--inflight'}"
					role="listitem"
				>
					<span class="sa-event-time">{formatTimestamp(event.createdAt)}</span>

					<span class="chip-backing">
						<abbr
							class="sa-event-outcome"
							data-augmented-ui="tl-clip br-clip border"
							style={outcomeStyle(event.outcome)}
							title={outcomeKey ? OUTCOME_TOOLTIPS[outcomeKey] : OUTCOME_TOOLTIPS['inflight']}
						>
							{outcomeKey ? OUTCOME_LABELS[outcomeKey] : 'In progress'}
						</abbr>
					</span>

					{#if event.description}
						<span class="sa-event-brief">{event.description}</span>
					{/if}

					<span class="sa-event-spacer" aria-hidden="true"></span>

					<span class="chip-backing">
						<button
							type="button"
							class="sa-event-jump"
							data-augmented-ui="tr-clip bl-clip border"
							onclick={() => onJumpTo?.(event.requestId)}
							aria-label="Jump to request #{event.requestId}"
						>
							<ArrowUpRight size={10} aria-hidden="true" /> #{event.requestId}
						</button>
					</span>
				</div>
			{/each}
		</div>
	{/if}
</div>

<style>
	.sa-card {
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

	.sa-head {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px;
	}

	.sa-type {
		font-family: var(--font-mono, monospace);
		font-size: 13px;
		font-weight: 600;
	}

	.sa-model {
		--aug-tl: 6px;
		--aug-br: 6px;
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-2);
		background: linear-gradient(0deg, var(--surface-2), var(--surface-2)), var(--surface-card);
		padding: 0 5px;
		text-decoration: none;
		border-bottom: 1px dotted var(--border-default);
		cursor: help;
	}

	.sa-spacer {
		flex: 1 1 auto;
	}

	.sa-count {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
	}

	.sa-desc {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		line-height: 1.55;
		color: var(--text-1);
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.sa-expand-row {
		display: flex;
	}

	.sa-expand-btn {
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

	.sa-expand-btn:hover {
		color: var(--text-strong);
	}

	.sa-expand-btn:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.sa-timeline {
		display: flex;
		flex-direction: column;
		gap: 2px;
		border-top: 1px solid var(--border-subtle);
		padding-top: 6px;
		margin-top: 2px;
	}

	.sa-event {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 6px;
		padding: 4px 0;
		border-bottom: 1px solid oklch(from var(--border-subtle) l c h / 0.5);
	}

	.sa-event:last-child {
		border-bottom: none;
	}

	.sa-event-time {
		flex: 0 0 44px;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-3);
		font-variant-numeric: tabular-nums;
		align-self: center;
	}

	.sa-event-outcome {
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

	.sa-event-brief {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-2);
		overflow-wrap: break-word;
		word-break: break-word;
		flex: 1 1 200px;
	}

	.sa-event-spacer {
		flex: 1 1 auto;
	}

	.sa-event-jump {
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

	.sa-event-jump:hover {
		background:
			linear-gradient(0deg, var(--interactive-hover-surface), var(--interactive-hover-surface)),
			var(--surface-card);
		--aug-border-bg: var(--border-default);
	}

	.sa-event-jump:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 1px;
	}
</style>
