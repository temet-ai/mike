<script lang="ts">
	import { Inbox } from '@lucide/svelte';
	import { getContext, untrack } from 'svelte';
	import { resolve } from '$app/paths';
	import { getSessionThreads } from '$lib/comms/api';
	import {
		describeDelivery,
		toneBadgeClass,
		type DeliveryTone
	} from '$lib/comms/deliveryPresentation';
	import type { ThreadSummaryDto } from '$lib/comms/types';
	import { MAILBOX_STORE_KEY } from '$lib/context-keys';
	import { createSessionMailboxStore } from '$lib/stores/mailbox.svelte';
	import type { SessionMailboxStore } from '$lib/stores/mailbox.svelte';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import { formatRelative } from '$lib/time';
	import { ArrowRight } from '@lucide/svelte';

	interface Props {
		sessionId: string;
		now: number;
		grouped?: boolean;
	}

	let { sessionId, now, grouped = false }: Props = $props();

	const contextStore = getContext<SessionMailboxStore | undefined>(MAILBOX_STORE_KEY);
	const store: SessionMailboxStore = contextStore ?? createSessionMailboxStore();

	$effect(() => {
		const sid = sessionId;
		untrack(() => {
			if (!sid) return;
			void store.load(sid);
		});
	});

	const forCurrentSession = $derived(store.currentId === sessionId);
	const envelope = $derived(forCurrentSession ? store.envelope : null);
	const loading = $derived(!forCurrentSession || store.loading);
	const loaded = $derived(forCurrentSession && store.loaded);
	const error = $derived(forCurrentSession ? store.error : null);

	interface DeliveryBucket {
		label: string;
		tone: DeliveryTone;
		count: number;
	}

	const DELIVERY_LABEL_ORDER = [
		'Queued',
		'Retrying',
		'Waiting a while',
		'Delivered',
		'Read',
		'Delivery stopped'
	];

	const deliveryBuckets = $derived.by((): DeliveryBucket[] => {
		const buckets = new Map<string, DeliveryBucket>();
		for (const m of [...(envelope?.inbox ?? []), ...(envelope?.outbox ?? [])]) {
			const presentation = describeDelivery(m, now);
			const existing = buckets.get(presentation.label);
			if (existing) {
				existing.count += 1;
			} else {
				buckets.set(presentation.label, {
					label: presentation.label,
					tone: presentation.tone,
					count: 1
				});
			}
		}
		return DELIVERY_LABEL_ORDER.map((label) => buckets.get(label)).filter(
			(b): b is DeliveryBucket => b !== undefined
		);
	});
	const totalCount = $derived((envelope?.inbox?.length ?? 0) + (envelope?.outbox?.length ?? 0));

	function retry(): void {
		void store.reload();
	}

	const NEWEST_THREADS_LIMIT = 3;
	let threads = $state<ThreadSummaryDto[]>([]);
	let threadsLoaded = $state(false);

	$effect(() => {
		const sid = sessionId;
		untrack(() => {
			threads = [];
			threadsLoaded = false;
			if (!sid) return;
			getSessionThreads(sid)
				.then((res) => {
					if (sessionId !== sid) return;
					threads = [...res.threads]
						.sort((a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime())
						.slice(0, NEWEST_THREADS_LIMIT);
					threadsLoaded = true;
				})
				.catch(() => {
					if (sessionId !== sid) return;
					threads = [];
					threadsLoaded = true;
				});
		});
	});

	const collapseKey = $derived(`session:${sessionId}:mailbox-open`);
	const commsHref = $derived(
		resolve(`/sessions/${encodeURIComponent(sessionId)}/comms` as `/sessions/${string}/comms`)
	);
</script>

<SectionCard
	title="Mailbox"
	titleIcon={Inbox}
	ariaLabel="Session mailbox summary"
	collapsible
	bordered={!grouped}
	sectionKey={collapseKey}
	defaultCollapsed
	titleTooltip="Delivery-status counts for every message this session has sent or received, plus its newest threads. Read the full inbox, threads, and transcripts on the comms page."
>
	{#snippet headerActions()}
		{#if loaded && !error}
			<span class="mbx-meta">{totalCount} {totalCount === 1 ? 'message' : 'messages'}</span>
		{/if}
	{/snippet}
	{#snippet children()}
		<div class="mbx-panel-body">
			{#if loading && !loaded}
				<div class="mbx-skeleton" aria-hidden="true">
					<div class="mbx-skel-row mbx-skel-row--short"></div>
				</div>
			{/if}

			{#if error}
				<LoadErrorBanner
					title="Couldn't load mailbox"
					detail={error}
					hint="The mailbox endpoint may not be live yet, or this session has no messages."
					onRetry={retry}
				/>
			{/if}

			{#if loaded && !error}
				{#if totalCount === 0}
					<p class="mbx-empty">No messages exchanged yet</p>
				{:else}
					<ul class="mbx-counts" role="list">
						{#each deliveryBuckets as bucket (bucket.label)}
							<li class="badge-nx {toneBadgeClass(bucket.tone)}">
								{bucket.label}
								{bucket.count}
							</li>
						{/each}
					</ul>
				{/if}

				{#if threadsLoaded && threads.length > 0}
					<div class="mbx-threads">
						<span class="mbx-threads__title">Newest threads</span>
						<ul class="mbx-thread-list" role="list">
							{#each threads as t (t.thread_uuid)}
								<li class="mbx-thread-row">
									<span class="mbx-thread-row__subject">{t.subject}</span>
									<span class="mbx-thread-row__time">{formatRelative(t.last_message_at, now)}</span>
								</li>
							{/each}
						</ul>
					</div>
				{/if}
			{/if}

			<a class="mbx-open-comms" href={commsHref} data-augmented-ui="tl-clip br-clip border">
				Open comms
				<ArrowRight size={13} aria-hidden="true" />
			</a>
		</div>
	{/snippet}
</SectionCard>

<style>
	.mbx-meta {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption, 11px);
		color: var(--text-3);
		white-space: nowrap;
	}

	.mbx-panel-body {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.mbx-empty {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--text-3);
		padding: 2px 0 4px;
	}

	.mbx-counts {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}

	.mbx-threads {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.mbx-threads__title {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
	}

	.mbx-thread-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.mbx-thread-row {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 4px 6px;
		border-radius: 2px;
		background: var(--surface-card);
	}

	.mbx-thread-row__subject {
		flex: 1;
		min-width: 0;
		overflow-wrap: anywhere;
		font-size: 12px;
		color: var(--text-1);
	}

	.mbx-thread-row__time {
		flex: none;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-3);
	}

	.mbx-open-comms {
		--edge: var(--edge-sm);
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		display: inline-flex;
		align-self: flex-start;
		align-items: center;
		gap: 6px;
		padding: 5px 12px;
		background: var(--surface-2);
		color: var(--text-2);
		text-decoration: none;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		font-weight: 600;
		transition: color var(--dur-1) var(--ease-snap);
	}

	.mbx-open-comms:hover {
		--aug-border-bg: var(--interactive-active-border);
		color: var(--text-strong);
	}

	.mbx-open-comms:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.mbx-skeleton {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.mbx-skel-row--short {
		height: 32px;
		border-radius: 2px;
		background: linear-gradient(
			90deg,
			var(--surface-2) 0%,
			var(--surface-shine) 50%,
			var(--surface-2) 100%
		);
		background-size: 200% 100%;
		opacity: 0.7;
	}

	.mbx-skel-row--short {
		animation: mbx-shimmer 1.6s ease-in-out infinite;
	}

	@keyframes mbx-shimmer {
		0% {
			background-position: 200% 0;
		}
		100% {
			background-position: -200% 0;
		}
	}
</style>
