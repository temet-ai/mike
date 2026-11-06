<script lang="ts">
	import { ArrowUpRight, Inbox, MessagesSquare } from '@lucide/svelte';
	import { resolve } from '$app/paths';
	import { shortSessionId } from '$lib/format';
	import CommsAvatar from './CommsAvatar.svelte';
	import type { PeerSummary } from './types';

	interface Props {
		peers: PeerSummary[];
		selectedPeer: string | null;
		onSelect: (sessionId: string | null) => void;
	}

	const { peers, selectedPeer, onSelect }: Props = $props();

	const STATE_LABEL: Record<PeerSummary['identity']['state'], string | null> = {
		working: 'Working',
		idle: 'Idle',
		stale: 'Stale',
		done: 'Done',
		error: 'Error',
		unknown: null
	};

	function peerLabel(peer: PeerSummary): string {
		return (
			peer.identity.label ??
			peer.identity.role ??
			peer.identity.agentType ??
			shortSessionId(peer.sessionId)
		);
	}

	function peerRole(peer: PeerSummary): string | null {
		if (!peer.identity.label) return null;
		return peer.identity.role ?? peer.identity.agentType;
	}

	function peerAriaLabel(peer: PeerSummary): string {
		const unread = peer.unconsumedCount > 0 ? `, ${peer.unconsumedCount} unread` : '';
		const state = STATE_LABEL[peer.identity.state];
		const stateBit = state ? ` — ${state}` : '';
		return `${peerLabel(peer)} (${shortSessionId(peer.sessionId)})${stateBit}, ${peer.messageCount} message${peer.messageCount === 1 ? '' : 's'}${unread}`;
	}

	function peerMeta(peer: PeerSummary): string {
		const role = peerRole(peer);
		const parts = [role, STATE_LABEL[peer.identity.state], `${peer.messageCount} msg`].filter(
			(p): p is string => p !== null
		);
		return parts.join(' · ');
	}
</script>

<div class="peer-strip" role="group" aria-label="Sessions this session has exchanged mail with">
	{#each peers as peer (peer.sessionId)}
		{@const active = selectedPeer === peer.sessionId}
		<span class="peer-row" class:peer-row--active={active}>
			<button
				type="button"
				class="peer-row__select"
				aria-pressed={active}
				aria-label={peerAriaLabel(peer)}
				onclick={() => onSelect(active ? null : peer.sessionId)}
				title={peer.sessionId}
			>
				<CommsAvatar
					sessionUuid={peer.sessionId}
					label={peer.identity.label}
					role={peer.identity.role}
					agentType={peer.identity.agentType}
					size="compact"
				/>
				<span class="peer-row__text">
					<span class="peer-row__line1">
						<span class="peer-row__name">{peerLabel(peer)}</span>
						{#if peer.unconsumedCount > 0}
							<span class="badge-nx badge-warning peer-row__mail">
								<Inbox size={10} aria-hidden="true" />
								{peer.unconsumedCount}
							</span>
						{/if}
					</span>
					<span class="peer-row__line2">
						<span class="font-mono">{shortSessionId(peer.sessionId)}</span>
						<span class="peer-row__dot" aria-hidden="true">·</span>
						<span>{peerMeta(peer)}</span>
					</span>
				</span>
			</button>
			<span class="peer-row__links">
				<a
					class="peer-row__link"
					href={resolve(`/sessions/${encodeURIComponent(peer.sessionId)}` as `/sessions/${string}`)}
					aria-label={`Open session detail for ${peerLabel(peer)}`}
					title="Session detail"
				>
					<ArrowUpRight size={11} aria-hidden="true" />
				</a>
				<a
					class="peer-row__link"
					href={resolve(
						`/sessions/${encodeURIComponent(peer.sessionId)}/comms` as `/sessions/${string}/comms`
					)}
					aria-label={`Open comms for ${peerLabel(peer)}`}
					title="Open comms"
				>
					<MessagesSquare size={11} aria-hidden="true" />
				</a>
			</span>
		</span>
	{/each}
	{#if peers.length === 0}
		<p class="peer-strip__empty">No mail exchanged with any other session yet</p>
	{/if}
</div>

<style>
	.peer-strip {
		display: flex;
		flex-wrap: nowrap;
		align-items: stretch;
		gap: 8px;
		overflow-x: auto;
	}

	.peer-strip__empty {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
	}

	.peer-row {
		flex: none;
		display: inline-flex;
		align-items: stretch;
		border-radius: 6px;
		background: transparent;
		transition: background var(--dur-1) var(--ease-snap);
	}

	.peer-row:hover {
		background: var(--surface-2);
	}

	.peer-row--active {
		background: var(--interactive-active-bg, var(--surface-2));
	}

	.peer-row__select {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
		padding: 5px 8px;
		border: none;
		border-radius: 6px 0 0 6px;
		background: transparent;
		cursor: pointer;
		text-align: left;
	}

	.peer-row__select:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}

	.peer-row__text {
		display: flex;
		flex-direction: column;
		gap: 1px;
		min-width: 0;
		max-width: 220px;
	}

	.peer-row__line1 {
		display: flex;
		align-items: center;
		gap: 6px;
		min-width: 0;
	}

	.peer-row__name {
		overflow-wrap: anywhere;
		font-family: var(--font-display);
		font-weight: 600;
		font-size: 12px;
		line-height: 1.3;
		color: var(--text-2);
	}

	.peer-row--active .peer-row__name {
		color: var(--text-strong);
	}

	.peer-row__mail {
		flex: none;
		font-variant-numeric: tabular-nums;
	}

	.peer-row__line2 {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		column-gap: 4px;
		overflow-wrap: anywhere;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-faint);
	}

	.peer-row__dot {
		color: var(--text-faint);
	}

	.peer-row__links {
		display: inline-flex;
		align-items: stretch;
		flex: none;
		border-left: 1px solid var(--border-subtle);
	}

	.peer-row__link {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 22px;
		flex: none;
		color: var(--text-3);
		text-decoration: none;
		transition: color var(--dur-1) var(--ease-snap);
	}

	.peer-row__link:hover {
		color: var(--text-strong);
	}

	.peer-row__link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}
</style>
