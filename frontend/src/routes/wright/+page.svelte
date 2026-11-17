<!--
@component
Wright: what is wrong in mike now and what was wrong before, what Wright did about each, what a
fix changed, and the conversation beside it. Design: artifacts/2026-10-16/resident/PAGE_SPEC.md;
API: artifacts/2026-11-14/wright/BUILD_PLAN.md.
-->
<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { replaceState } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { Button } from '$lib/components/ui/button';
	import { Select } from '$lib/components/ui/select';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import WrightActionRow from '$lib/components/wright/WrightActionRow.svelte';
	import WrightBand from '$lib/components/wright/WrightBand.svelte';
	import WrightCauseRow from '$lib/components/wright/WrightCauseRow.svelte';
	import WrightConversation from '$lib/components/wright/WrightConversation.svelte';
	import WrightNote from '$lib/components/wright/WrightNote.svelte';
	import WrightFixRow from '$lib/components/wright/WrightFixRow.svelte';
	import WrightFollowupRow from '$lib/components/wright/WrightFollowupRow.svelte';
	import WrightProblemDetail from '$lib/components/wright/WrightProblemDetail.svelte';
	import WrightProblemRow from '$lib/components/wright/WrightProblemRow.svelte';
	import type { WrightProblem } from '$lib/api/wright';
	import type { Feed } from '$lib/wright/feed';
	import {
		hashWithoutPair,
		pairTokenFromHash,
		readOperatorToken,
		storeOperatorToken
	} from '$lib/wright/pairing';
	import { causeState } from '$lib/wright/causes';
	import { FOLLOWUP_FILTERS, groupByDay } from '$lib/wright/present';
	import { createWrightStore } from '$lib/wright/store.svelte';
	import { proxyEvents } from '$lib/ws.svelte';

	const store = createWrightStore();

	let token = $state<string | null>(null);
	let requestedProblem = $state<number | null>(null);
	const paired = $derived(token !== null);

	const problems = $derived<WrightProblem[]>([...store.open.rows, ...store.earlier.rows]);
	const problemById = (id: number): WrightProblem | undefined => problems.find((p) => p.id === id);
	/** The representative problem of a cause: from the lists, or fetched when its row was opened. */
	const causeProblem = (id: number): WrightProblem | undefined =>
		problemById(id) ?? store.fetchedProblems[id];
	const waitingCauses = $derived(
		store.causes.rows.filter((c) => causeState(c, store.waitedHours) === 'waiting').length
	);
	const causeMeta = $derived(
		`${waitingCauses > 0 ? `${waitingCauses} waiting · ` : ''}${store.causes.rows.length} ${store.causes.rows.length === 1 ? 'cause' : 'causes'}`
	);
	const stuckFollowups = $derived(store.followups.rows.filter((f) => f.state === 'stuck').length);
	const followupMeta = $derived(
		stuckFollowups > 0
			? `${stuckFollowups} stuck · ${store.followups.rows.length} listed`
			: `${store.followups.rows.length} listed`
	);
	const actionDays = $derived(groupByDay(store.actions.rows, (a) => a.created_at));

	function readHash(): void {
		const match = /^#problem-(\d+)$/.exec(window.location.hash);
		requestedProblem = match === null ? null : Number(match[1]);
	}

	function takePairing(): void {
		const pairToken = pairTokenFromHash(window.location.hash);
		if (pairToken !== null) {
			storeOperatorToken(pairToken);
			const clean = `${window.location.pathname}${window.location.search}${hashWithoutPair(window.location.hash)}`;
			try {
				replaceState(clean, {});
			} catch {
				history.replaceState(history.state, '', clean);
			}
		}
		token = readOperatorToken();
	}

	onMount(() => {
		takePairing();
		readHash();
		void store.loadAll();
		const stop = store.startLive();
		const onVisible = (): void => {
			if (document.visibilityState === 'visible') void store.refresh();
		};
		document.addEventListener('visibilitychange', onVisible);
		window.addEventListener('hashchange', readHash);
		return () => {
			stop();
			document.removeEventListener('visibilitychange', onVisible);
			window.removeEventListener('hashchange', readHash);
		};
	});

	// A reconnect reloads the real state: what arrived while the socket was down is not lost.
	let hadConnection = false;
	let dropped = false;
	$effect(() => {
		const state = proxyEvents.state;
		if (state === 'connected') {
			if (dropped) untrack(() => void store.loadAll());
			hadConnection = true;
			dropped = false;
		} else if (hadConnection) {
			dropped = true;
		}
	});
</script>

{#snippet problemList(feed: Feed<WrightProblem>, empty: string, onMore: () => void)}
	{#if feed.status === 'loading'}
		<WrightNote>Loading.</WrightNote>
	{:else if feed.status === 'error'}
		<LoadErrorBanner
			title="Could not load problems"
			detail={feed.error ?? ''}
			onRetry={() => void store.loadAll()}
		/>
	{:else if feed.status === 'missing'}
		<WrightNote>Not in the running mike yet.</WrightNote>
	{:else if feed.rows.length === 0}
		<WrightNote>{empty}</WrightNote>
	{:else}
		<ul class="wr-rows" role="list">
			{#each feed.rows as problem (problem.id)}
				<WrightProblemRow
					{problem}
					requestOpen={requestedProblem === problem.id}
					onOpen={(id) => void store.loadDetail(id)}
				>
					<WrightProblemDetail
						{problem}
						detail={store.details[problem.id]}
						undo={store.undo}
						onUndo={(id) => void store.undoAction(id)}
						onMore={(which) => void store.moreDetail(problem.id, which)}
						onRetry={() => void store.loadDetail(problem.id)}
					/>
				</WrightProblemRow>
			{/each}
		</ul>
		{#if feed.cursor !== null}
			<Button variant="outline" size="sm" disabled={feed.loadingMore} onclick={onMore}>
				Load more
			</Button>
		{/if}
	{/if}
{/snippet}

{#snippet causeList()}
	{#if store.causes.status === 'loading'}
		<WrightNote>Loading.</WrightNote>
	{:else if store.causes.status === 'missing'}
		<WrightNote>Not in the running mike yet.</WrightNote>
	{:else if store.causes.status === 'error'}
		<LoadErrorBanner
			title="Could not load the triage queue"
			detail={store.causes.error ?? ''}
			onRetry={() => void store.loadAll()}
		/>
	{:else if store.causes.rows.length === 0}
		<WrightNote>Nothing to triage. Every open problem has an answer.</WrightNote>
	{:else}
		<ul class="wr-rows" role="list">
			{#each store.causes.rows as cause (cause.cause_id)}
				{@const problem = causeProblem(cause.problem_id)}
				<WrightCauseRow
					{cause}
					waitedHours={store.waitedHours}
					onOpen={(id) => void store.loadDetail(id)}
				>
					{#if problem !== undefined}
						<WrightProblemDetail
							{problem}
							detail={store.details[problem.id]}
							undo={store.undo}
							onUndo={(id) => void store.undoAction(id)}
							onMore={(which) => void store.moreDetail(problem.id, which)}
							onRetry={() => void store.loadDetail(problem.id)}
						/>
					{:else}
						<WrightNote>Loading.</WrightNote>
					{/if}
				</WrightCauseRow>
			{/each}
		</ul>
	{/if}
{/snippet}

{#snippet followupList()}
	{#if store.followups.status === 'loading'}
		<WrightNote>Loading.</WrightNote>
	{:else if store.followups.status === 'missing'}
		<WrightNote>Not in the running mike yet.</WrightNote>
	{:else if store.followups.status === 'error'}
		<LoadErrorBanner
			title="Could not load follow-ups"
			detail={store.followups.error ?? ''}
			onRetry={() => void store.setFollowupFilter(store.followupFilter)}
		/>
	{:else if store.followups.rows.length === 0}
		<WrightNote>
			{store.followupFilter === 'all'
				? 'Nothing filed yet. Wright adds a follow-up each time its engineer finds something to change.'
				: 'No follow-ups in that state.'}
		</WrightNote>
	{:else}
		<ul class="wr-rows" role="list">
			{#each store.followups.rows as followup (followup.id)}
				<WrightFollowupRow {followup} limits={store.followupLimits} />
			{/each}
		</ul>
		{#if store.followups.cursor !== null}
			<Button
				variant="outline"
				size="sm"
				disabled={store.followups.loadingMore}
				onclick={() => void store.moreFollowups()}
			>
				Load more
			</Button>
		{/if}
	{/if}
{/snippet}

<div class="wr-scroll">
	<section class="wr" aria-label="Wright">
		<PageIntro
			title="Wright"
			level="h1"
			description="Wright finds what goes wrong in sessions, mail and fleets, fixes it, and tells you what it did. Ask it about anything here, or tell it what to leave alone."
		/>
		{#if store.watchEnabled === false}
			<WrightNote role="status">
				Wright is off. Turn it on in <a href={resolve('/settings')}>settings</a>.
			</WrightNote>
		{/if}

		<div class="wr-body">
			<section class="wr-convo" aria-labelledby="wr-convo-h">
				<h2 class="machine wr-col-h" id="wr-convo-h">Conversation</h2>
				<WrightConversation
					messages={store.messages.rows}
					status={store.messages.status}
					error={store.messages.error}
					pendingReply={store.pendingReply}
					{paired}
					sending={store.sending}
					sendError={store.sendError}
					undo={store.undo}
					onSend={(text) => store.send(text)}
					onUndoSteer={(message) => void store.undoSteer(message)}
					onRetry={() => void store.loadAll()}
				/>
			</section>

			<section class="wr-doing" aria-labelledby="wr-doing-h">
				<h2 class="machine wr-col-h" id="wr-doing-h">What Wright is doing</h2>
				<div class="edge mech-group wr-stack">
					<WrightBand
						id="triage"
						label="Triage queue"
						meta={causeMeta}
						held={store.causes.held.length}
						onShowHeld={store.showHeldCauses}
					>
						{@render causeList()}
					</WrightBand>

					<WrightBand
						id="open"
						label="Open now"
						meta="{store.open.total ?? store.open.rows.length} open"
						held={store.open.held.length}
						onShowHeld={store.showHeldOpen}
					>
						{@render problemList(
							store.open,
							'Nothing open. Wright checks every hour, and straight away when something fails.',
							() => void store.moreOpen()
						)}
					</WrightBand>

					<WrightBand
						id="followups"
						label="Follow-ups"
						meta={followupMeta}
						held={store.followups.held.length}
						onShowHeld={store.showHeldFollowups}
					>
						<div class="wr-filter">
							<Select
								label="Show"
								value={store.followupFilter}
								options={FOLLOWUP_FILTERS}
								onchange={(event) =>
									void store.setFollowupFilter(
										event.currentTarget.value as typeof store.followupFilter
									)}
							/>
						</div>
						{@render followupList()}
					</WrightBand>

					<WrightBand
						id="earlier"
						label="Healthy again"
						meta={`${store.earlier.total ?? store.earlier.rows.length} ${(store.earlier.total ?? store.earlier.rows.length) === 1 ? 'problem' : 'problems'}`}
						held={store.earlier.held.length}
						onShowHeld={store.showHeldEarlier}
						defaultOpen={false}
					>
						{@render problemList(
							store.earlier,
							'Nothing has healed yet.',
							() => void store.moreEarlier()
						)}
					</WrightBand>

					<WrightBand
						id="did"
						label="What it did"
						meta="{store.actions.rows.length} listed"
						held={store.actions.held.length}
						onShowHeld={store.showHeldActions}
					>
						{#if store.actions.status === 'loading'}
							<WrightNote>Loading.</WrightNote>
						{:else if store.actions.status === 'missing'}
							<WrightNote
								>Not in the running mike yet. Wright's known fixes appear here once they are
								deployed.</WrightNote
							>
						{:else if store.actions.status === 'error'}
							<LoadErrorBanner
								title="Could not load what Wright did"
								detail={store.actions.error ?? ''}
								onRetry={() => void store.loadAll()}
							/>
						{:else if store.actions.rows.length === 0}
							<WrightNote>Nothing done yet.</WrightNote>
						{:else}
							{#each actionDays as group (group.day)}
								<p class="machine wr-subday">{group.day}</p>
								<ul class="wr-rows" role="list">
									{#each group.rows as action (action.id)}
										<WrightActionRow
											{action}
											problem={problemById(action.problem_id)}
											undo={store.undo[`action-${action.id}`]}
											onUndo={(id) => void store.undoAction(id)}
										/>
									{/each}
								</ul>
							{/each}
							{#if store.actions.cursor !== null}
								<Button
									variant="outline"
									size="sm"
									disabled={store.actions.loadingMore}
									onclick={() => void store.moreActions()}
								>
									Load earlier
								</Button>
							{/if}
						{/if}
					</WrightBand>

					<WrightBand
						id="fixes"
						label="Known fixes"
						meta={`${store.remedies.rows.length}`}
						defaultOpen={false}
					>
						{#if store.remedies.status === 'loading'}
							<WrightNote>Loading.</WrightNote>
						{:else if store.remedies.status === 'missing'}
							<WrightNote>Not in the running mike yet.</WrightNote>
						{:else if store.remedies.status === 'error'}
							<LoadErrorBanner
								title="Could not load known fixes"
								detail={store.remedies.error ?? ''}
								onRetry={() => void store.loadAll()}
							/>
						{:else if store.remedies.rows.length === 0}
							<WrightNote
								>None yet. Wright adds one each time it solves a problem that will come back.</WrightNote
							>
						{:else}
							<ul class="wr-rows" role="list">
								{#each store.remedies.rows as remedy (remedy.name)}
									<WrightFixRow {remedy} actions={store.actions.rows} />
								{/each}
							</ul>
						{/if}
					</WrightBand>
				</div>
			</section>
		</div>
	</section>
</div>

<style>
	.wr-scroll {
		min-height: 0;
		background: var(--surface-bg);
		container-type: inline-size;
	}

	.wr {
		display: flex;
		flex-direction: column;
		gap: var(--space-5);
		max-width: 1600px;
		padding: var(--space-5) var(--space-6) var(--space-8);
	}

	.wr-body {
		display: grid;
		grid-template-columns: minmax(0, 5fr) minmax(22rem, 4fr);
		gap: var(--space-5);
		align-items: start;
	}

	.wr-convo {
		position: sticky;
		top: var(--space-4);
		min-width: 0;
	}

	.wr-doing {
		min-width: 0;
		display: flex;
		flex-direction: column;
	}

	.wr-col-h {
		margin: 0 0 var(--space-2);
		color: var(--text-3);
	}

	.wr-stack {
		--edge-line: var(--border-subtle);
		--edge-surface: var(--surface-card);
	}

	.wr-rows {
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.wr-filter {
		inline-size: 14rem;
	}

	.wr-subday {
		margin: var(--space-2) 0 0;
		color: var(--text-3);
	}

	@container (max-width: 860px) {
		.wr {
			padding: var(--space-4) var(--space-4) var(--space-6);
		}

		.wr-body {
			grid-template-columns: minmax(0, 1fr);
		}

		.wr-convo {
			position: static;
		}
	}
</style>
