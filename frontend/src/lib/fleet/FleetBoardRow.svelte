<script lang="ts">
	import { Inbox, MessagesSquare } from '@lucide/svelte';
	import { resolve } from '$app/paths';
	import { shortSessionId } from '$lib/format';
	import { formatClockAbsolute, formatRelative } from '$lib/time';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import {
		canvasStatePillClass,
		errorLineFor,
		errorLineIsAgent as isAgentError,
		type CanvasNode
	} from './graphNormalize';
	import { roleTokens } from './roleTokens';

	interface Props {
		node: CanvasNode;
		selected: boolean;
		dimmed: boolean;
		hasComms: boolean;
		now: number;
		onSelect: (id: string) => void;
	}

	const { node, selected, dimmed, hasComms, now, onSelect }: Props = $props();

	const tokens = $derived(roleTokens(node.role));
	const Icon = $derived(tokens.icon);
	const pillClass = $derived(canvasStatePillClass(node.state));

	const errorLine = $derived(errorLineFor(node));
	const errorLineIsAgent = $derived(isAgentError(node));
</script>

<div class="flb-row-wrap">
	<button
		type="button"
		class="flb-row"
		class:flb-row--selected={selected}
		class:flb-row--dimmed={dimmed}
		onclick={() => onSelect(node.id)}
		aria-pressed={selected}
		aria-label={`Inspect ${node.label}`}
		data-slot="fleet-board-row"
		data-node-id={node.id}
		data-node-state={node.state}
	>
		<span class="flb-row__pill-slot">
			<span class="status-pill {pillClass}" data-slot="fleet-board-row-state"
				>{node.stateLabel}</span
			>
		</span>

		<span class="flb-row__main">
			<span class="flb-row__title" class:flb-row__title--id={!node.hasIdentityLabel}
				>{node.label}</span
			>
			{#if errorLine !== null}
				<span
					class="flb-row__reason"
					class:flb-row__reason--muted={!errorLineIsAgent}
					data-slot="fleet-board-row-reason"
				>
					{errorLine}
				</span>
			{/if}
		</span>

		<span class="flb-row__role" data-slot="fleet-board-row-role">
			<span class="flb-row__role-icon" style:color={tokens.fg}
				><Icon size={12} aria-hidden="true" /></span
			>
			<span class:flb-row__role-text--unknown={node.role === null && node.agentType === null}>
				{tokens.label}
			</span>
		</span>

		{#if node.quality !== null}
			{@const quality = node.quality}
			<Tooltip text={quality.rationale}>
				{#snippet child({ props })}
					<span
						{...props}
						class="badge-nx badge-muted flb-row__quality"
						data-slot="fleet-board-row-quality"
					>
						judge · {quality.outcome}
					</span>
				{/snippet}
			</Tooltip>
		{/if}

		{#if node.unconsumedMail > 0}
			<Tooltip
				text={`${node.unconsumedMail} unconsumed mailbox ${node.unconsumedMail === 1 ? 'message' : 'messages'}`}
			>
				{#snippet child({ props })}
					<span {...props} class="badge-nx badge-warning flb-row__mail">
						<Inbox size={11} aria-hidden="true" />
						{node.unconsumedMail}
					</span>
				{/snippet}
			</Tooltip>
		{/if}

		<span class="flb-row__count">
			{node.requestCount ?? '—'}
			{node.requestCount === 1 ? 'req' : 'reqs'}
		</span>

		<span
			class="flb-row__time"
			title={node.lastActivityAt ? formatClockAbsolute(node.lastActivityAt) : ''}
		>
			{node.lastActivityAt ? formatRelative(node.lastActivityAt, now) : '—'}
		</span>

		<span class="flb-row__id font-mono">{shortSessionId(node.id)}</span>
	</button>

	<a
		class="flb-row__comms-link"
		class:flb-row__comms-link--active={hasComms}
		href={resolve(`/sessions/${encodeURIComponent(node.id)}/comms` as `/sessions/${string}/comms`)}
		aria-label={hasComms
			? `Open comms for ${node.label} (has exchanged mail)`
			: `Open comms for ${node.label}`}
		title={hasComms ? 'Has exchanged mail — open comms' : 'Open comms'}
	>
		<MessagesSquare size={13} aria-hidden="true" />
	</a>
</div>

<style>
	.flb-row-wrap {
		display: flex;
		align-items: stretch;
		width: 100%;
		border-top: 1px solid var(--border-subtle);
	}
	.flb-row-wrap:first-child {
		border-top: none;
	}

	.flb-row {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 10px;
		row-gap: 6px;
		flex: 1;
		min-width: 0;
		padding: 8px 14px;
		border: none;
		background: var(--surface-card);
		text-align: left;
		cursor: pointer;
		transition:
			border-color 0.14s,
			background 0.14s,
			opacity 0.16s;
	}
	.flb-row:hover {
		background: var(--surface-2);
	}
	.flb-row:focus-visible {
		outline: none;
		box-shadow: inset 0 0 0 2px var(--ring);
	}
	.flb-row--selected {
		background: var(--surface-2);
		box-shadow: inset 2px 0 0 var(--interactive-active-border);
	}
	.flb-row--dimmed {
		filter: saturate(0.4) brightness(0.75);
	}

	.flb-row__pill-slot {
		flex: none;
		width: 62px;
	}
	.flb-row__pill-slot .status-pill {
		white-space: nowrap;
	}

	.flb-row__main {
		flex: 1;
		min-width: 160px;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.flb-row__title {
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-weight: 600;
		font-size: 12.5px;
		color: var(--text-strong);
		overflow-wrap: break-word;
	}
	.flb-row__title--id {
		font-weight: 400;
		color: var(--text-3);
	}
	.flb-row__reason {
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 11px;
		color: var(--status-danger-fg);
		overflow-wrap: break-word;
	}
	.flb-row__reason--muted {
		color: var(--text-2);
	}

	.flb-row__role {
		flex: none;
		display: inline-flex;
		align-items: center;
		gap: 5px;
		width: 140px;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 10.5px;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--text-3);
		overflow: hidden;
	}
	.flb-row__role-icon {
		display: inline-flex;
		flex: none;
	}
	.flb-row__role-text--unknown {
		color: var(--text-faint);
		font-style: italic;
		text-transform: none;
		letter-spacing: normal;
	}

	.flb-row__quality {
		flex: none;
		white-space: nowrap;
	}
	.flb-row__mail {
		flex: none;
		font-variant-numeric: tabular-nums;
	}

	.flb-row__count {
		flex: none;
		width: 64px;
		text-align: right;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 11px;
		color: var(--text-2);
		font-variant-numeric: tabular-nums;
	}
	.flb-row__time {
		flex: none;
		width: 68px;
		text-align: right;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 11px;
		color: var(--text-2);
		font-variant-numeric: tabular-nums;
	}
	.flb-row__id {
		flex: none;
		width: 84px;
		text-align: right;
		font-size: 10.5px;
		color: var(--text-faint);
	}

	.flb-row__comms-link {
		flex: none;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 34px;
		background: var(--surface-card);
		color: var(--text-faint);
		text-decoration: none;
		transition:
			color 0.14s,
			background 0.14s;
	}
	.flb-row__comms-link:hover {
		background: var(--surface-2);
		color: var(--text-strong);
	}
	.flb-row__comms-link:focus-visible {
		outline: none;
		box-shadow: inset 0 0 0 2px var(--ring);
	}
	.flb-row__comms-link--active {
		color: var(--text-2);
	}
</style>
