<script lang="ts">
	import { Inbox } from '@lucide/svelte';
	import { shortSessionId } from '$lib/format';
	import { formatClockAbsolute, formatRelative } from '$lib/time';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import {
		canvasStateBorderVar,
		canvasStatePillClass,
		errorLineFor,
		errorLineIsAgent,
		type CanvasNode
	} from './graphNormalize';
	import { roleTokens } from './roleTokens';

	interface Props {
		node: CanvasNode;
		x: number;
		y: number;
		selected: boolean;
		dimmed: boolean;
		now: number;
		onSelect: (id: string) => void;
	}

	const { node, x, y, selected, dimmed, now, onSelect }: Props = $props();

	const tokens = $derived(roleTokens(node.role));
	const Icon = $derived(tokens.icon);
	const pillClass = $derived(canvasStatePillClass(node.state));
	const stateBorder = $derived(canvasStateBorderVar(node.state));

	const titleText = $derived(node.title ?? node.goal);
	const hasTitleText = $derived(titleText !== null);

	const errorLine = $derived(errorLineFor(node));
	const errorAgent = $derived(errorLineIsAgent(node));
	const quality = $derived(node.quality);

	const relativeTime = $derived(
		node.lastActivityAt ? formatRelative(node.lastActivityAt, now) : '—'
	);
	const absoluteTime = $derived(
		node.lastActivityAt ? formatClockAbsolute(node.lastActivityAt) : undefined
	);

	const ariaLabel = $derived(
		`Inspect ${node.label} · ${node.stateLabel}${errorLine !== null ? ` · ${errorLine}` : ''}`
	);
</script>

<button
	type="button"
	class="fleet-node"
	class:fleet-node--selected={selected}
	class:fleet-node--dimmed={dimmed}
	style:left="{x}px"
	style:top="{y}px"
	style:--role-fg={tokens.fg}
	style:--role-bg={tokens.bg}
	style:--role-border={tokens.border}
	style:--state-border={stateBorder}
	onclick={() => onSelect(node.id)}
	aria-pressed={selected}
	aria-label={ariaLabel}
	data-slot="fleet-node"
	data-node-id={node.id}
	data-node-state={node.state}
	data-node-role={node.role ?? 'unknown'}
	data-augmented-ui="tl-clip br-clip border"
>
	<span class="fleet-node__z1">
		<span class="fleet-node__icon"><Icon size={13} /></span>
		<span
			class="fleet-node__role"
			class:fleet-node__role--unknown={tokens.label === 'role unknown'}
		>
			{tokens.label}
		</span>
		<span class="status-pill {pillClass} fleet-node__state" data-slot="fleet-node-state">
			{node.stateLabel}
		</span>
	</span>

	<span
		class="fleet-node__z2"
		class:fleet-node__z2--muted={!hasTitleText}
		title={titleText ?? undefined}
	>
		{titleText ?? 'no title reported'}
	</span>

	<span class="fleet-node__z3" class:fleet-node__z3--empty={errorLine === null && quality === null}>
		{#if errorLine !== null}
			<span
				class="fleet-node__error"
				class:fleet-node__error--muted={!errorAgent}
				title={errorLine}
			>
				{errorLine}
			</span>
		{:else if quality !== null}
			<Tooltip text={quality.rationale}>
				{#snippet child({ props })}
					<span
						{...props}
						class="badge-nx badge-muted fleet-node__quality"
						data-slot="fleet-node-quality"
					>
						judge · {quality.outcome}
					</span>
				{/snippet}
			</Tooltip>
		{/if}
	</span>

	<span class="fleet-node__z4">
		<span class="fleet-node__chip font-mono">{shortSessionId(node.sessionId)}</span>
		{#if node.unconsumedMail > 0}
			<Tooltip
				text={`${node.unconsumedMail} unconsumed mailbox ${node.unconsumedMail === 1 ? 'message' : 'messages'}`}
			>
				{#snippet child({ props })}
					<span {...props} class="badge-nx badge-warning fleet-node__mail">
						<Inbox size={10} aria-hidden="true" />
						{node.unconsumedMail}
					</span>
				{/snippet}
			</Tooltip>
		{/if}
		<span class="fleet-node__time" title={absoluteTime}>{relativeTime}</span>
	</span>
</button>

<style>
	.fleet-node {
		--edge: 6px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1.5px;
		--aug-border-bg: var(--state-border);
		position: absolute;
		width: 220px;
		height: 128px;
		transform: translate(-50%, -50%);
		display: flex;
		flex-direction: column;
		gap: 6px;
		background: var(--surface-card);
		padding: calc(var(--edge) + 4px);
		box-shadow:
			inset 0 1px 0 var(--surface-shine),
			var(--shadow-sm);
		text-align: left;
		cursor: pointer;
		transition:
			box-shadow 0.16s,
			opacity 0.16s;
	}
	.fleet-node:hover {
		box-shadow:
			inset 0 1px 0 var(--surface-shine),
			var(--shadow-md);
	}
	.fleet-node:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--ring);
	}
	.fleet-node--selected {
		box-shadow:
			0 0 0 3px var(--surface-2),
			0 0 0 5px var(--interactive-active-border),
			var(--shadow-md);
	}
	.fleet-node--dimmed {
		filter: saturate(0.4) brightness(0.75);
	}

	.fleet-node__z1 {
		flex: none;
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.fleet-node__icon {
		flex: none;
		display: grid;
		place-items: center;
		width: 18px;
		height: 18px;
		border-radius: 6px;
		background: var(--role-bg);
		color: var(--role-fg);
		border: 1px solid var(--role-border);
	}
	.fleet-node__role {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		white-space: nowrap;
		text-overflow: ellipsis;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: var(--text-caption);
		letter-spacing: 0.09em;
		text-transform: uppercase;
		color: var(--text-3);
	}
	.fleet-node__role--unknown {
		text-transform: none;
		letter-spacing: normal;
		font-style: italic;
		color: var(--text-faint);
	}
	.fleet-node__state {
		flex: none;
	}

	.fleet-node__z2 {
		flex: none;
		max-height: calc(2 * 1.3 * 13px);
		overflow: hidden;
		overflow-wrap: break-word;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-weight: 600;
		font-size: 13px;
		line-height: 1.3;
		color: var(--text-strong);
	}
	.fleet-node__z2--muted {
		font-weight: 400;
		font-style: italic;
		color: var(--text-faint);
	}

	.fleet-node__z3 {
		flex: 1;
		min-height: 0;
		display: flex;
		align-items: flex-start;
	}
	.fleet-node__z3--empty {
		visibility: hidden;
	}
	.fleet-node__error {
		overflow: hidden;
		white-space: nowrap;
		text-overflow: ellipsis;
		width: 100%;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: var(--text-caption);
		color: var(--status-danger-fg);
	}
	.fleet-node__error--muted {
		color: var(--text-2);
	}
	.fleet-node__quality {
		flex: none;
	}

	.fleet-node__z4 {
		flex: none;
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.fleet-node__chip {
		flex: none;
		border: 1px solid var(--border-subtle);
		border-radius: 100px;
		padding: 1px 8px;
		font-size: var(--text-caption);
		color: var(--text-3);
	}
	.fleet-node__mail {
		flex: none;
		font-variant-numeric: tabular-nums;
	}
	.fleet-node__time {
		flex: none;
		margin-left: auto;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: var(--text-caption);
		color: var(--text-2);
		font-variant-numeric: tabular-nums;
	}
</style>
