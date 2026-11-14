<script lang="ts">
	import { X } from '@lucide/svelte';
	import { resolve } from '$app/paths';
	import CopyIconButton from '$lib/components/shared/CopyIconButton.svelte';
	import { formatRelative } from '$lib/time';
	import { canvasStatePillClass, connectionsFor } from './graphNormalize';
	import type { CanvasEdge, CanvasNode } from './graphNormalize';
	import { roleTokens } from './roleTokens';
	import SessionIdentityCard from '$lib/comms/SessionIdentityCard.svelte';
	import SessionNotebook from '$lib/comms/SessionNotebook.svelte';
	import FleetAcceptance from './FleetAcceptance.svelte';
	import SeatDecisions from './SeatDecisions.svelte';
	import {
		getFleetAcceptanceForSession,
		getFleetDecisionModes,
		getFleetRecord,
		getLaunchSeat
	} from './api';
	import { seatDecisionFigures, type SeatDecisionFigures } from './seatDecisions';
	import type { FleetAcceptanceStatus, FleetDecisionModes } from './types';

	interface Props {
		node: CanvasNode | null;
		edges: CanvasEdge[];
		nodesById: Map<string, CanvasNode>;
		now: number;
		onSelect: (id: string | null) => void;
	}

	const { node, edges, nodesById, now, onSelect }: Props = $props();

	const tokens = $derived(node ? roleTokens(node.role) : null);
	const Icon = $derived(tokens ? tokens.icon : null);
	const pillClass = $derived(node ? canvasStatePillClass(node.state) : null);
	const connections = $derived(node ? connectionsFor(node.id, edges, nodesById) : []);
	const acceptanceSessionId = $derived(node?.sessionId ?? null);
	let acceptanceStatus = $state<FleetAcceptanceStatus | null>(null);
	let acceptanceLoading = $state(false);
	let acceptanceError = $state<string | null>(null);
	let acceptanceRequest = 0;

	async function loadAcceptance(sessionId: string, request: number): Promise<void> {
		acceptanceLoading = true;
		acceptanceError = null;
		try {
			const status = await getFleetAcceptanceForSession(sessionId);
			if (request === acceptanceRequest) acceptanceStatus = status;
		} catch (error) {
			if (request === acceptanceRequest) {
				acceptanceError = error instanceof Error ? error.message : String(error);
			}
		} finally {
			if (request === acceptanceRequest) acceptanceLoading = false;
		}
	}

	$effect(() => {
		const sessionId = acceptanceSessionId;
		const request = ++acceptanceRequest;
		acceptanceStatus = null;
		acceptanceError = null;
		acceptanceLoading = false;
		if (sessionId) void loadAcceptance(sessionId, request);
		return () => {
			acceptanceRequest += 1;
		};
	});

	function refreshAcceptance(): void {
		if (node) void loadAcceptance(node.sessionId, ++acceptanceRequest);
	}

	const decisionsFleet = $derived(acceptanceStatus?.fleet ?? null);
	let decisionsFigures = $state<SeatDecisionFigures | null>(null);
	let decisionsModes = $state<FleetDecisionModes | null>(null);
	let decisionsLoading = $state(false);
	let decisionsError = $state<string | null>(null);
	let decisionsReason = $state<string | null>(null);
	let decisionsShown = $state(false);
	let decisionsRequest = 0;

	async function loadDecisions(sessionId: string, fleet: string, request: number): Promise<void> {
		decisionsLoading = true;
		decisionsError = null;
		try {
			const [seat, record, modes] = await Promise.all([
				getLaunchSeat(sessionId),
				getFleetRecord(fleet),
				getFleetDecisionModes(fleet)
			]);
			if (request !== decisionsRequest) return;
			decisionsModes = modes;
			decisionsReason = record === null ? 'Figures arrive when the run closes.' : null;
			decisionsFigures = seatDecisionFigures(record, seat);
			decisionsShown =
				record === null || (seat !== null && record.effort_split?.by_seat?.[seat] !== undefined);
		} catch (error) {
			if (request === decisionsRequest) {
				decisionsError = error instanceof Error ? error.message : String(error);
				decisionsShown = true;
			}
		} finally {
			if (request === decisionsRequest) decisionsLoading = false;
		}
	}

	$effect(() => {
		const sessionId = acceptanceSessionId;
		const fleet = decisionsFleet;
		const request = ++decisionsRequest;
		decisionsFigures = null;
		decisionsModes = null;
		decisionsError = null;
		decisionsReason = null;
		decisionsShown = false;
		decisionsLoading = false;
		if (sessionId && fleet) void loadDecisions(sessionId, fleet, request);
		return () => {
			decisionsRequest += 1;
		};
	});

	function retryDecisions(): void {
		if (node && decisionsFleet)
			void loadDecisions(node.sessionId, decisionsFleet, ++decisionsRequest);
	}
</script>

<aside
	class="fleet-inspector"
	data-slot="fleet-inspector"
	aria-label="Node inspector"
	data-augmented-ui="tl-clip br-clip border"
	style:--aug-border-bg={tokens
		? `linear-gradient(135deg, ${tokens.border} 0%, var(--border-default) 55%)`
		: 'var(--border-default)'}
>
	{#if node === null || tokens === null || Icon === null || pillClass === null}
		<div class="fleet-inspector__empty" data-slot="fleet-inspector-empty">
			<div class="fleet-inspector__empty-big">no node selected</div>
			<div>Click any session to see what it is working on, its activity, and how it is linked.</div>
		</div>
	{:else}
		<div
			class="fleet-inspector__head"
			style:--role-fg={tokens.fg}
			style:--role-bg={tokens.bg}
			style:--role-border={tokens.border}
		>
			<div class="fleet-inspector__head-row">
				<span class="fleet-inspector__icon"><Icon size={19} /></span>
				<div class="fleet-inspector__id-block">
					<div class="fleet-inspector__name">{node.label}</div>
					<div class="fleet-inspector__role">{tokens.label}</div>
				</div>
				<span class="status-pill {pillClass}" data-slot="fleet-inspector-state"
					>{node.stateLabel}</span
				>
				<button
					type="button"
					class="fleet-inspector__close"
					onclick={() => onSelect(null)}
					aria-label="Close inspector"
					data-slot="fleet-inspector-close"
				>
					<X size={16} aria-hidden="true" />
				</button>
			</div>
			{#if node.stateDetail !== null || node.errorKind !== null}
				<div class="fleet-inspector__state-detail" data-slot="fleet-inspector-state-detail">
					{#if node.errorKind !== null && node.errorKind !== 'agent'}
						<span class="fleet-inspector__error-kind"
							>{node.errorKind === 'infra' ? 'infra event' : 'ended early'}</span
						>
					{/if}
					{node.stateDetail}
				</div>
			{/if}
			<div class="fleet-inspector__ids">
				<span class="fleet-inspector__session-id">
					<span class="fleet-inspector__k">session</span>
					{node.id}
					<CopyIconButton getText={() => node.id} label="Copy session id" />
				</span>
				{#if node.model}<span><span class="fleet-inspector__k">model</span> {node.model}</span>{/if}
				{#if node.agentType}<span data-slot="fleet-inspector-dispatched"
						><span class="fleet-inspector__k">last dispatched</span> {node.agentType}</span
					>{/if}
				{#if node.workspace}<span
						><span class="fleet-inspector__k">workspace</span> {node.workspace}</span
					>{/if}
				{#if node.cwd}<span><span class="fleet-inspector__k">cwd</span> {node.cwd}</span>{/if}
				{#if node.worktree}<span
						><span class="fleet-inspector__k">worktree</span> {node.worktree}</span
					>{/if}
			</div>
		</div>

		<div class="fleet-inspector__body">
			{#if acceptanceStatus || acceptanceLoading || acceptanceError}
				<div class="fleet-inspector__grp">
					<FleetAcceptance
						status={acceptanceStatus}
						loading={acceptanceLoading}
						error={acceptanceError}
						onRefresh={refreshAcceptance}
					/>
				</div>
			{/if}
			<div class="fleet-inspector__grp">
				<h4>Working on now</h4>
				<div class="fleet-inspector__task">{node.workingOnNow ?? '—'}</div>
			</div>

			<div class="fleet-inspector__grp" data-slot="fleet-inspector-activity">
				<h4>Recent activity</h4>
				{#if node.lastActivityAt !== null}
					<div class="fleet-inspector__log">
						{formatRelative(node.lastActivityAt, now)}
						{#if node.requestCount !== null}
							· {node.requestCount} request{node.requestCount === 1 ? '' : 's'}
						{/if}
						{#if node.unconsumedMail > 0}
							· {node.unconsumedMail} unconsumed mailbox {node.unconsumedMail === 1
								? 'message'
								: 'messages'}
						{/if}
					</div>
				{:else}
					<p class="fleet-inspector__none">no activity yet</p>
				{/if}
			</div>

			{#if node.quality !== null}
				<div class="fleet-inspector__grp" data-slot="fleet-inspector-quality">
					<h4>Quality</h4>
					<div class="fleet-inspector__task">
						<span class="badge-nx badge-muted">{node.quality.outcome}</span>
						<span class="fleet-inspector__quality-rationale">{node.quality.rationale}</span>
					</div>
				</div>
			{/if}

			{#if decisionsLoading || decisionsShown}
				<div class="fleet-inspector__grp" data-slot="fleet-inspector-decisions">
					<h4>Decisions</h4>
					<SeatDecisions
						seat={decisionsFigures}
						modes={decisionsModes}
						loading={decisionsLoading}
						error={decisionsError}
						onRetry={retryDecisions}
						unavailableReason={decisionsReason}
					/>
				</div>
			{/if}

			<div class="fleet-inspector__grp" data-slot="fleet-inspector-identity">
				<SessionIdentityCard sessionId={node.sessionId} fallbackLabel={node.label} />
			</div>

			<div class="fleet-inspector__grp" data-slot="fleet-inspector-notebook">
				<SessionNotebook sessionId={node.sessionId} />
			</div>

			<div class="fleet-inspector__grp">
				<h4>Files</h4>
				{#if node.files.length}
					<div class="fleet-inspector__files">
						{#each node.files as f (f)}<code>{f}</code>{/each}
					</div>
				{:else}
					<span class="fleet-inspector__none">none reported</span>
				{/if}
			</div>

			<div class="fleet-inspector__grp">
				<h4>Connections</h4>
				{#if connections.length}
					<div class="fleet-inspector__rel">
						{#each connections as c (c.other.id + c.direction + c.kind)}
							<button
								type="button"
								class="fleet-inspector__rrow"
								onclick={() => onSelect(c.other.id)}
							>
								<span class="fleet-inspector__dot" style:background={roleTokens(c.other.role).fg}
								></span>
								<span class="fleet-inspector__verb"
									>{c.direction === 'out' ? c.kind : `from · ${c.kind}`}</span
								>
								<span aria-hidden="true">{c.direction === 'out' ? '→' : '←'}</span>
								<span class="fleet-inspector__to font-mono">{c.other.label}</span>
							</button>
						{/each}
					</div>
				{:else}
					<span class="fleet-inspector__none">not linked</span>
				{/if}
			</div>
		</div>

		{#if node.sessionId}
			<div class="fleet-inspector__foot">
				<a
					class="fleet-inspector__lnk"
					href={resolve(`sessions/${encodeURIComponent(node.sessionId)}` as const)}
				>
					open captured traffic →
				</a>
				<a
					class="fleet-inspector__lnk"
					href={resolve(`sessions/${encodeURIComponent(node.sessionId)}/comms` as const)}
				>
					open comms →
				</a>
			</div>
		{/if}
	{/if}
</aside>

<style>
	.fleet-inspector {
		--edge-tl: 8px;
		--edge-br: 8px;
		--aug-tl: var(--edge-tl);
		--aug-br: var(--edge-br);
		--aug-border-all: 1px;
		width: 352px;
		flex: none;
		background: var(--surface-2);
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}
	@container fleet-workspace (max-width: 640px) {
		.fleet-inspector {
			width: 100%;
			flex: 1 1 0;
			min-height: 0;
		}
	}
	.fleet-inspector__empty {
		margin: auto;
		padding: 30px;
		text-align: center;
		color: var(--text-3);
		font-size: 13px;
		max-width: 230px;
	}
	.fleet-inspector__empty-big {
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-faint);
		font-size: 12px;
		letter-spacing: 0.1em;
		text-transform: uppercase;
		margin-bottom: 10px;
	}
	.fleet-inspector__head {
		padding: 16px 18px 14px;
		border-bottom: 1px solid var(--border-default);
	}
	.fleet-inspector__head-row {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.fleet-inspector__icon {
		width: 34px;
		height: 34px;
		border-radius: 9px;
		display: grid;
		place-items: center;
		background: var(--role-bg);
		color: var(--role-fg);
		border: 1px solid var(--role-border);
		flex: none;
	}
	.fleet-inspector__id-block {
		display: flex;
		min-width: 0;
		flex: 1;
		flex-direction: column;
	}
	.fleet-inspector__name {
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 700;
		font-size: 16px;
		letter-spacing: -0.01em;
		color: var(--text-strong);
		overflow-wrap: break-word;
	}
	.fleet-inspector__role {
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 10px;
		letter-spacing: 0.09em;
		text-transform: uppercase;
		color: var(--text-3);
		margin-top: 2px;
	}
	.fleet-inspector__close {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 26px;
		height: 26px;
		border-radius: 7px;
		flex: none;
		border: none;
		background: transparent;
		color: var(--text-3);
		cursor: pointer;
		transition:
			background 0.14s,
			color 0.14s;
	}
	.fleet-inspector__close:hover {
		background: var(--surface-card);
		color: var(--text-strong);
	}
	.fleet-inspector__close:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--ring);
	}
	.fleet-inspector__session-id {
		display: inline-flex;
		align-items: center;
		gap: 4px;
	}
	.fleet-inspector__state-detail {
		margin-top: 8px;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 11.5px;
		color: var(--status-danger-fg);
		overflow-wrap: break-word;
	}
	.fleet-inspector__error-kind {
		display: inline-block;
		margin-right: 6px;
		padding: 1px 6px;
		border-radius: 4px;
		background: var(--status-muted-bg);
		color: var(--status-muted-fg);
		font-size: 10px;
		letter-spacing: 0.05em;
		text-transform: uppercase;
	}
	.fleet-inspector__quality-rationale {
		display: block;
		margin-top: 6px;
		color: var(--text-2);
	}
	.fleet-inspector__ids {
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 11px;
		color: var(--text-3);
		margin-top: 12px;
		display: flex;
		gap: 8px;
		flex-wrap: wrap;
	}
	.fleet-inspector__k {
		color: var(--text-faint);
		margin-right: 3px;
	}
	.fleet-inspector__body {
		flex: 1;
		overflow-y: auto;
		padding: 16px 18px 26px;
	}
	@container fleet-workspace (max-width: 640px) {
		.fleet-inspector__body {
			min-height: 0;
		}
	}
	.fleet-inspector__grp {
		margin-bottom: 20px;
	}
	.fleet-inspector__grp h4 {
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 10px;
		letter-spacing: 0.12em;
		text-transform: uppercase;
		color: var(--text-3);
		margin: 0 0 8px;
		font-weight: 600;
		display: flex;
		align-items: baseline;
		gap: 8px;
	}
	.fleet-inspector__ts {
		font-weight: 400;
		text-transform: none;
		letter-spacing: normal;
		color: var(--text-faint);
	}
	.fleet-inspector__task {
		font-size: 13.5px;
		color: var(--text-1);
		line-height: 1.45;
		overflow-wrap: break-word;
	}
	.fleet-inspector__log {
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 11.5px;
		color: var(--text-1);
	}
	.fleet-inspector__none {
		color: var(--text-faint);
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 11.5px;
	}
	.fleet-inspector__files {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}
	.fleet-inspector__files code {
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 11px;
		background: var(--surface-card);
		border: 1px solid var(--border-default);
		border-radius: 5px;
		padding: 3px 8px;
		color: var(--text-1);
		overflow-wrap: break-word;
	}
	.fleet-inspector__rel {
		display: flex;
		flex-direction: column;
		gap: 7px;
	}
	.fleet-inspector__rrow {
		display: flex;
		align-items: center;
		gap: 9px;
		font-size: 12px;
		color: var(--text-3);
		cursor: pointer;
		padding: 6px 9px;
		border: 1px solid var(--border-default);
		border-radius: 7px;
		background: var(--surface-card);
		transition: border-color 0.15s;
		width: 100%;
		text-align: left;
	}
	.fleet-inspector__rrow:hover {
		border-color: var(--interactive-active-border);
	}
	.fleet-inspector__dot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		flex: none;
	}
	.fleet-inspector__verb {
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 9.5px;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--text-faint);
	}
	.fleet-inspector__to {
		font-size: 12px;
		color: var(--text-1);
		margin-left: auto;
		overflow-wrap: break-word;
	}
	.fleet-inspector__foot {
		border-top: 1px solid var(--border-default);
		padding: 12px 18px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.fleet-inspector__lnk {
		display: block;
		text-align: center;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 11px;
		color: var(--text-3);
		text-decoration: none;
		border: 1px solid var(--border-default);
		border-radius: 7px;
		padding: 8px;
		background: var(--surface-card);
		transition:
			border-color 0.15s,
			color 0.15s;
	}
	.fleet-inspector__lnk:hover {
		border-color: var(--interactive-active-border);
		color: var(--interactive-active-fg);
	}
</style>
