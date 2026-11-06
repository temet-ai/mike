<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { getContext } from 'svelte';
	import { MediaQuery } from 'svelte/reactivity';
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import { ArrowLeft } from '@lucide/svelte';
	import { shortSessionId } from '$lib/format';
	import { SESSION_DETAIL_STORE_KEY } from '$lib/context-keys';
	import type { SessionDetailStore } from '$lib/stores/cache/sessionDetailStore.svelte';
	import { COMMS_STORE_KEY, createCommsStore } from '$lib/comms/commsStore.svelte';
	import type { CommsStore } from '$lib/comms/commsStore.svelte';
	import { resolveParticipantLabel } from '$lib/comms/participantIdentity';
	import type { ParticipantIdentityDto } from '$lib/comms/types';
	import CommsFleetHeader from '$lib/comms/CommsFleetHeader.svelte';
	import CommsNewThread from '$lib/comms/CommsNewThread.svelte';
	import CommsPeerStrip from '$lib/comms/CommsPeerStrip.svelte';
	import CommsThreadList from '$lib/comms/CommsThreadList.svelte';
	import CommsTranscript from '$lib/comms/CommsTranscript.svelte';
	import TeamFeed from '$lib/comms/TeamFeed.svelte';
	import SessionIdentityCard from '$lib/comms/SessionIdentityCard.svelte';
	import SessionNotebook from '$lib/comms/SessionNotebook.svelte';
	import { Button } from '$lib/components/ui/button';
	import CopyIconButton from '$lib/components/shared/CopyIconButton.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import { ApiError } from '$lib/api';
	import {
		getTeamComms,
		isTeamCommsUnavailable,
		type TeamCommsResponse
	} from '$lib/comms/teamComms';

	const detailStore = getContext<SessionDetailStore | undefined>(SESSION_DETAIL_STORE_KEY);
	const commsContextStore = getContext<CommsStore | undefined>(COMMS_STORE_KEY);
	const comms: CommsStore = commsContextStore ?? createCommsStore();

	const sessionId = $derived(page.params.sessionId ?? '');
	const threadIdentities = $derived.by(() => {
		const map = new Map<string, ParticipantIdentityDto>();
		for (const t of comms.threads) {
			for (const p of t.participant_identities ?? []) {
				if (!map.has(p.session_uuid)) map.set(p.session_uuid, p);
			}
		}
		return map;
	});
	const resolvedSessionLabel = $derived(
		resolveParticipantLabel(sessionId || null, threadIdentities)
	);
	const sessionTitle = $derived(detailStore?.detail?.summary?.title || resolvedSessionLabel);
	const project = $derived(detailStore?.detail?.summary?.project ?? null);

	let now = $state(Date.now());

	let readingThread = $state(false);
	let transcriptPanel = $state<HTMLElement | null>(null);
	let listPanel = $state<HTMLElement | null>(null);
	const compactLayout = new MediaQuery('(max-width: 860px)');
	let threadTrigger: HTMLElement | null = null;

	async function selectThread(uuid: string): Promise<void> {
		if (compactLayout.current && document.activeElement instanceof HTMLElement)
			threadTrigger = document.activeElement;
		readingThread = true;
		void comms.selectThread(uuid);
		await tick();
		if (compactLayout.current) transcriptPanel?.focus();
	}

	async function showThreads(): Promise<void> {
		readingThread = false;
		await tick();
		if (threadTrigger?.isConnected) threadTrigger.focus();
		else listPanel?.focus();
	}

	let teamComms = $state<TeamCommsResponse | null>(null);
	let teamCommsLoading = $state(true);
	let teamCommsError = $state<string | null>(null);
	let teamCommsUnavailable = $state(false);

	async function loadTeamComms(id: string): Promise<void> {
		teamCommsLoading = true;
		teamCommsError = null;
		teamCommsUnavailable = false;
		try {
			teamComms = await getTeamComms(id);
		} catch (err) {
			if (isTeamCommsUnavailable(err)) {
				teamCommsUnavailable = true;
				teamComms = null;
			} else {
				teamCommsError = err instanceof ApiError ? err.message : String(err);
			}
		} finally {
			teamCommsLoading = false;
		}
	}

	const mailboxEmpty = $derived(
		comms.threadsLoaded && !comms.threadsLoading && comms.threads.length === 0
	);
	const teamCommsHasData = $derived(
		teamComms !== null && (teamComms.dispatches.length > 0 || teamComms.messages.length > 0)
	);

	$effect(() => {
		const id = sessionId;
		const proj = project;
		if (!id) return;
		void comms.load(id, proj);
	});

	$effect(() => {
		void sessionId;
		readingThread = false;
	});

	$effect(() => {
		const id = sessionId;
		if (!id) return;
		void loadTeamComms(id);
	});

	onMount(() => {
		const timer = setInterval(() => (now = Date.now()), 1_000);
		return () => clearInterval(timer);
	});
</script>

<svelte:head>
	<title>Comms — {sessionTitle} — mike</title>
</svelte:head>

<section class="col detail comms-page" aria-label="Session comms">
	<header class="comms-header">
		<span class="comms-back-backing">
			<a
				href={resolve(`/sessions/${encodeURIComponent(sessionId)}` as `/sessions/${string}`)}
				class="comms-back"
				data-augmented-ui="tr-clip bl-clip border"
				aria-label="Back to session"
			>
				<ArrowLeft size={14} aria-hidden="true" />
			</a>
		</span>
		<div class="comms-header__titles">
			<h2 class="comms-title">{sessionTitle} — Comms</h2>
			<span class="comms-subtitle">
				<span class="font-mono">{shortSessionId(sessionId)}</span>
				<CopyIconButton
					getText={() => sessionId}
					label="Copy session id"
					copiedLabel="Copied"
					size={11}
				/>
			</span>
		</div>
		<CommsFleetHeader
			{sessionId}
			{project}
			dto={comms.fleetDto}
			unavailable={comms.fleetUnavailable}
		/>
	</header>

	<div class="comms-identity">
		<SessionIdentityCard {sessionId} fallbackLabel={resolvedSessionLabel} />
	</div>

	<div class="comms-peer-band">
		<span class="panel-hd comms-peer-band__label">Peers</span>
		<CommsPeerStrip
			peers={comms.peers}
			selectedPeer={comms.filterParticipant}
			onSelect={comms.setFilterParticipant}
		/>
	</div>

	{#if comms.threadsUnavailable}
		<p class="comms-unavailable">Thread comms aren't live for this session yet.</p>
	{:else if comms.threadsError}
		<LoadErrorBanner
			title="Couldn't load threads"
			detail={comms.threadsError}
			onRetry={() => void comms.refreshThreads()}
		/>
	{:else if mailboxEmpty}
		<div class="comms-body comms-body--empty">
			<CommsNewThread
				{sessionId}
				{project}
				peers={comms.peers}
				legacyPhase={comms.newThreadPhase}
				legacyError={comms.newThreadError}
				onLegacySend={(to, subject, body, tags) =>
					void comms.startNewThread(to, subject, body, tags)}
				onOpened={(uuid) => {
					void comms.refreshThreads();
					void comms.selectThread(uuid);
				}}
			/>
			{#if teamCommsHasData}
				<p class="comms-empty-note">
					This session coordinates {teamComms?.dispatches.length ?? 0} agent{(teamComms?.dispatches
						.length ?? 0) === 1
						? ''
						: 's'} — see <a href="#comms-team-feed">Team activity</a> below. No mailbox threads yet.
				</p>
			{:else}
				<p class="comms-empty-note">
					No messages yet for this session. Start a thread above — a subject and a message is all it
					takes; add sessions to notify only if you want to alert specific peers.
				</p>
			{/if}
		</div>
	{:else}
		<div class="comms-body" class:comms-body--reading={readingThread}>
			<div class="comms-body__list" data-reveal bind:this={listPanel} tabindex="-1">
				<CommsNewThread
					{sessionId}
					{project}
					peers={comms.peers}
					legacyPhase={comms.newThreadPhase}
					legacyError={comms.newThreadError}
					onLegacySend={(to, subject, body, tags) =>
						void comms.startNewThread(to, subject, body, tags)}
					onOpened={(uuid) => {
						void comms.refreshThreads();
						void selectThread(uuid);
					}}
				/>
				<CommsThreadList
					threads={comms.filteredThreads}
					tags={comms.tags}
					selectedThreadUuid={comms.selectedThreadUuid}
					filterTag={comms.filterTag}
					filterParticipant={comms.filterParticipant}
					filterResolved={comms.filterResolved}
					{now}
					loading={comms.threadsLoading || !comms.threadsLoaded}
					onSelect={(uuid) => void selectThread(uuid)}
					onFilterTag={comms.setFilterTag}
					onClearParticipant={() => comms.setFilterParticipant(null)}
					onFilterResolved={comms.setFilterResolved}
				/>
			</div>
			<div class="comms-body__transcript" data-reveal bind:this={transcriptPanel} tabindex="-1">
				<div class="comms-mobile-back">
					<Button variant="outline" onclick={() => void showThreads()}>
						<ArrowLeft size={16} aria-hidden="true" /> All threads
					</Button>
				</div>
				<CommsTranscript
					{sessionId}
					thread={comms.selectedThread}
					messages={comms.messages}
					loading={comms.threadLoading}
					error={comms.threadError}
					{now}
					peers={comms.peers}
					replyPhase={comms.replyPhase}
					replyError={comms.replyError}
					replyNotice={comms.replyNotice}
					ackErrors={comms.ackErrors}
					resolvePhase={comms.resolvePhase}
					onSendReply={(body, options) => void comms.sendReply(body, options)}
					onResolve={() => void comms.resolveSelected()}
					onAck={(uuid) => void comms.ack(uuid)}
					onOpenThread={(uuid) => void selectThread(uuid)}
				/>
			</div>
		</div>
	{/if}

	<div id="comms-team-feed" class="comms-team-feed">
		<TeamFeed
			{now}
			data={teamComms}
			loading={teamCommsLoading}
			error={teamCommsError}
			unavailable={teamCommsUnavailable}
			onRetry={() => void loadTeamComms(sessionId)}
		/>
		<div class="comms-notebook">
			<SessionNotebook {sessionId} />
		</div>
	</div>
</section>

<style>
	.comms-page {
		display: flex;
		flex-direction: column;
		gap: 14px;
	}

	.comms-header {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 12px 24px 0;
		flex-shrink: 0;
	}

	.comms-back-backing {
		display: inline-flex;
		flex-shrink: 0;
		background: var(--surface-bg);
	}

	.comms-back {
		--edge: 6px;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		color: var(--text-2);
		text-decoration: none;
		transition: color var(--dur-1) var(--ease-snap);
	}

	.comms-back:hover {
		--aug-border-bg: var(--border-default);
		color: var(--text-strong);
	}

	.comms-back:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.comms-header__titles {
		display: flex;
		flex-direction: column;
		gap: 2px;
		flex: 1;
		min-width: 0;
	}

	.comms-title {
		margin: 0;
		font-family: var(--font-display);
		font-size: 1.05rem;
		font-weight: 700;
		letter-spacing: -0.01em;
		color: var(--text-1);
		overflow-wrap: break-word;
	}

	.comms-subtitle {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: 11px;
		color: var(--text-3);
	}

	.comms-identity {
		padding: 0 24px;
		flex-shrink: 0;
	}

	.comms-notebook {
		margin-top: 14px;
	}

	.comms-peer-band {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 0 24px;
		flex-shrink: 0;
	}

	.comms-peer-band__label {
		flex: none;
	}

	.comms-peer-band > :global(.peer-strip) {
		flex: 1;
		min-width: 0;
	}

	.comms-unavailable {
		margin: 0 24px;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--text-3);
	}

	.comms-body {
		display: grid;
		grid-template-columns: minmax(280px, 380px) 1fr;
		align-items: start;
		gap: 16px;
		padding: 0 24px 16px;
	}

	.comms-body__list {
		position: sticky;
		top: 44px;
		max-height: calc(100vh - 44px);
		overflow-y: auto;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.comms-body__transcript {
		min-width: 0;
	}

	.comms-body--empty {
		display: flex;
		flex-direction: column;
		gap: 10px;
		max-width: 480px;
	}

	.comms-mobile-back {
		display: none;
	}

	.comms-empty-note {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		line-height: 1.6;
		color: var(--text-2);
	}

	.comms-empty-note a {
		color: var(--status-info-fg);
	}

	@media (max-width: 860px) {
		.comms-header {
			flex-wrap: wrap;
		}

		.comms-header__titles {
			flex-basis: 220px;
			min-width: 220px;
		}

		.comms-body {
			grid-template-columns: 1fr;
		}

		.comms-body__list {
			position: static;
			max-height: none;
			overflow: visible;
		}

		.comms-mobile-back {
			display: block;
		}

		.comms-body:not(.comms-body--reading) .comms-body__transcript,
		.comms-body--reading .comms-body__list {
			display: none;
		}
	}

	.comms-team-feed {
		padding: 0 24px 20px;
	}
</style>
