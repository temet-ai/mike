<script lang="ts">
	import { getContext, onMount, untrack } from 'svelte';
	import { ArrowLeft } from '@lucide/svelte';
	import { resolve } from '$app/paths';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import { Button } from '$lib/components/ui/button';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import { formatCount } from '$lib/time';
	import CommsThreadFilters from './CommsThreadFilters.svelte';
	import CommsThreadRows from './CommsThreadRows.svelte';
	import CommsTranscript from './CommsTranscript.svelte';
	import { fleetThreadsFirst } from './groupThreads';
	import {
		GLOBAL_COMMS_STORE_KEY,
		createGlobalCommsStore,
		type GlobalCommsStore
	} from './globalComms.svelte';

	interface Props {
		projectPath: string;
		nested?: boolean;
	}

	const { projectPath, nested = false }: Props = $props();

	const liveStore = getContext<GlobalCommsStore | undefined>(GLOBAL_COMMS_STORE_KEY);
	const store = createGlobalCommsStore({ project: untrack(() => projectPath) });

	let now = $state(Date.now());

	onMount(() => {
		const release = store.retain();
		void store.load();
		const unsubscribe = liveStore?.subscribe(store.handleEvent);
		const timer = setInterval(() => (now = Date.now()), 5_000);
		return () => {
			unsubscribe?.();
			clearInterval(timer);
			release();
		};
	});

	const sectionKey = $derived(`project:${projectPath}:comms`);
	const threads = $derived(fleetThreadsFirst(store.threads));
	const hasActiveFilter = $derived(
		store.filterTag !== null || store.filterParticipant !== null || store.filterResolved !== 'all'
	);
	const heading = $derived(
		store.threadsLoaded && !store.threadsError && !store.threadsUnavailable
			? `Comms · ${formatCount(store.threads.length)} ${store.threads.length === 1 ? 'thread' : 'threads'}`
			: 'Comms'
	);
	const openHref = $derived.by(() => {
		const params = new URLSearchParams({ project: projectPath });
		if (store.selectedThreadUuid) params.set('thread', store.selectedThreadUuid);
		return resolve(`/comms?${params.toString()}` as `/comms?${string}`);
	});

	function clearFilters(): void {
		store.setFilterTag(null);
		store.setFilterParticipant(null);
		store.setFilterResolved('all');
	}
</script>

<div class="pcm-scope" data-slot="project-comms">
	<SectionCard
		title={heading}
		collapsible
		bordered={!nested}
		{sectionKey}
		defaultCollapsed={false}
		titleTooltip="Conversations between this project's sessions. Fleet threads come first."
	>
		{#snippet headerActions()}
			<a class="pcm-open" href={openHref} data-slot="project-comms-open-link">Open in Comms</a>
		{/snippet}
		{#snippet children()}
			{#if store.threadsUnavailable}
				<p class="pcm-note">Thread comms aren't live yet.</p>
			{:else if store.threadsError}
				<LoadErrorBanner
					title="Couldn't load threads"
					detail={store.threadsError}
					onRetry={() => void store.refreshThreads()}
				/>
			{:else if store.selectedThreadUuid}
				<div class="pcm-thread" data-slot="project-comms-thread">
					<div class="pcm-back">
						<Button variant="outline" size="sm" onclick={() => void store.selectThread(null)}>
							<ArrowLeft size={14} aria-hidden="true" /> All threads
						</Button>
					</div>
					<CommsTranscript
						sessionId=""
						readOnly
						subjectLevel="h3"
						thread={store.selectedThread}
						messages={store.messages}
						loading={store.threadLoading}
						error={store.threadError}
						{now}
						peers={[]}
						replyPhase="idle"
						replyError={null}
						replyNotice={null}
						resolvePhase={store.resolvePhase}
						onSendReply={() => {}}
						onResolve={() => void store.resolveSelected()}
						onAck={() => {}}
						onOpenThread={(uuid) => void store.selectThread(uuid)}
					/>
				</div>
			{:else}
				<div class="pcm-list" data-slot="project-comms-list">
					<CommsThreadFilters
						tags={store.tags}
						filterTag={store.filterTag}
						filterParticipant={store.filterParticipant}
						filterResolved={store.filterResolved}
						onFilterTag={store.setFilterTag}
						onClearParticipant={() => store.setFilterParticipant(null)}
						onFilterResolved={store.setFilterResolved}
					/>
					{#if store.threadsLoading && !store.threadsLoaded}
						<p class="pcm-note" role="status">Loading threads</p>
					{:else if threads.length === 0}
						{#if hasActiveFilter}
							<p class="pcm-note">
								No threads match the current filters.
								<button type="button" class="pcm-clear" onclick={clearFilters}>Clear filters</button
								>
							</p>
						{:else}
							<p class="pcm-note">No threads in this project yet.</p>
						{/if}
					{:else}
						<CommsThreadRows
							{threads}
							selectedThreadUuid={store.selectedThreadUuid}
							{now}
							onSelect={(uuid) => void store.selectThread(uuid)}
						/>
					{/if}
				</div>
			{/if}
		{/snippet}
	</SectionCard>
</div>

<style>
	.pcm-scope {
		min-width: 0;
	}

	.pcm-open {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption);
		color: var(--status-info-fg);
		text-decoration: underline;
		text-underline-offset: 2px;
		white-space: nowrap;
	}

	.pcm-open:focus-visible,
	.pcm-clear:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.pcm-list,
	.pcm-thread {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		min-width: 0;
	}

	.pcm-back {
		display: flex;
	}

	.pcm-note {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-sm);
		line-height: var(--leading-body);
		color: var(--text-3);
	}

	.pcm-clear {
		padding: 0;
		border: none;
		background: none;
		color: var(--status-info-fg);
		font: inherit;
		text-decoration: underline;
		text-underline-offset: 2px;
		cursor: pointer;
	}
</style>
