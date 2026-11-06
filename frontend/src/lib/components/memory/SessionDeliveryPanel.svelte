<script lang="ts">
	import { PackageCheck, RefreshCw } from '@lucide/svelte';
	import { getContext } from 'svelte';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import SessionDeliveryProfile from '$lib/components/memory/SessionDeliveryProfile.svelte';
	import SessionDeliveryContent from '$lib/components/memory/SessionDeliveryContent.svelte';
	import SessionDeliveryStages from '$lib/components/memory/SessionDeliveryStages.svelte';
	import { Button } from '$lib/components/ui/button';
	import { ApiError } from '$lib/api';
	import { getSessionDelivery, type SessionDelivery } from '$lib/api/sessionDelivery';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import { createRetryBackoff } from '$lib/retryBackoff';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';

	interface Props {
		sessionId: string;
		grouped?: boolean;
	}

	let { sessionId, grouped = false }: Props = $props();

	const collapseStore = getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY);
	const sectionKey = $derived(`session:${sessionId}:delivery-open`);
	const sectionExpanded = $derived(collapseStore ? collapseStore.isExpanded(sectionKey) : false);

	let data = $state<SessionDelivery | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let attemptedForSession = $state<string | null>(null);
	let statusAnnouncement = $state('');

	const backoff = createRetryBackoff();

	async function load(id: string): Promise<void> {
		backoff.cancel();
		loading = true;
		error = null;
		try {
			const result = await getSessionDelivery(id);
			if (id !== sessionId) return;
			data = result;
			backoff.succeeded();
			statusAnnouncement = `Loaded: ${result.lessons.length} lessons, ${result.topics.length} topics and ${result.skills.length} skills for this session.`;
		} catch (err) {
			if (id !== sessionId) return;
			error = err instanceof ApiError ? err.message : String(err);
			statusAnnouncement = "Couldn't load what was delivered to this session.";
			backoff.failed(() => {
				if (id === sessionId) void load(id);
			});
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		const id = sessionId;
		if (!sectionExpanded) return;
		if (attemptedForSession === id) return;
		attemptedForSession = id;
		backoff.reset();
		void load(id);
	});

	$effect(() => backoff.cancel);

	function reload(): void {
		backoff.reset();
		void load(sessionId);
	}
</script>

<SectionCard
	title="What mike delivered"
	titleIcon={PackageCheck}
	collapsible
	bordered={!grouped}
	{sectionKey}
	defaultCollapsed
	titleTooltip="The profile this session ran under, the lessons, topics and skills it was given, and the record that each reached a request or was loaded."
>
	{#snippet headerActions()}
		{#if data}
			<span class="sdl-meta" data-slot="session-delivery-summary"
				>{data.lessons.length} lessons · {data.skills.length} skills · {data.topics.length} topics</span
			>
		{/if}
	{/snippet}
	{#snippet children()}
		<span class="sr-only" role="status" aria-live="polite">{statusAnnouncement}</span>
		<div class="sdl-body" data-slot="session-delivery-panel">
			{#if loading && !data}
				<div class="sdl-skeleton" aria-hidden="true">
					<div class="sdl-skel-row"></div>
					<div class="sdl-skel-row sdl-skel-row--short"></div>
				</div>
			{/if}
			{#if error}
				<LoadErrorBanner
					title="Couldn't load what was delivered"
					detail={error}
					hint="The session delivery endpoint may not be reachable yet."
					onRetry={reload}
				/>
			{:else if data}
				<SessionDeliveryProfile {data} />
				<SessionDeliveryContent {sessionId} {data} />
				<SessionDeliveryStages {sessionId} {data} />
				<div class="sdl-foot">
					<Button variant="ghost" size="sm" onclick={reload} disabled={loading}>
						<RefreshCw size={12} aria-hidden="true" />
						Reload
					</Button>
				</div>
			{/if}
		</div>
	{/snippet}
</SectionCard>

<style>
	.sdl-meta {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption, 11px);
		color: var(--text-3);
		white-space: nowrap;
	}
	.sdl-body {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}
	.sdl-foot {
		display: flex;
		justify-content: flex-end;
	}
	.sdl-skeleton {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.sdl-skel-row {
		height: 14px;
		border-radius: var(--r-tol);
		background: var(--surface-2);
	}
	.sdl-skel-row--short {
		width: 60%;
	}
</style>
