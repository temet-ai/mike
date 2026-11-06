<script lang="ts">
	import { getContext } from 'svelte';
	import { History, Info } from '@lucide/svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import DecisionActionCard from '$lib/components/memory/DecisionActionCard.svelte';
	import { ApiError } from '$lib/api';
	import {
		getSessionMemoryActions,
		restoreLesson,
		type ConsolidationAction
	} from '$lib/api/memoryDecisions';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import { createRetryBackoff } from '$lib/retryBackoff';

	interface Props {
		sessionId: string;
		grouped?: boolean;
	}

	let { sessionId, grouped = false }: Props = $props();

	const collapseStore = getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY);
	const sectionKey = $derived(`session:${sessionId}:memory-actions-open`);
	let sectionExpandedLocal = $state(false);
	const sectionExpanded = $derived(
		collapseStore ? collapseStore.isExpanded(sectionKey) : sectionExpandedLocal
	);
	function toggleSection(): void {
		if (collapseStore) collapseStore.toggle(sectionKey);
		else sectionExpandedLocal = !sectionExpandedLocal;
	}

	let actions = $state<ConsolidationAction[]>([]);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let attemptedForSession = $state<string | null>(null);

	const backoff = createRetryBackoff();

	async function load(id: string): Promise<void> {
		backoff.cancel();
		loading = true;
		error = null;
		try {
			const result = await getSessionMemoryActions(id);
			if (id !== sessionId) return;
			actions = result;
			backoff.succeeded();
		} catch (err) {
			if (id !== sessionId) return;
			error = err instanceof ApiError ? err.message : String(err);
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

	function retry(): void {
		backoff.reset();
		void load(sessionId);
	}

	const headingId = $derived(`sma-heading-${sessionId}`);

	let restoringActionId = $state<number | null>(null);
	let restoreErrors = $state<Record<number, string>>({});
	let restoredActionIds = $state<Set<number>>(new Set());
	let restoreAnnouncement = $state('');

	async function handleRestore(action: ConsolidationAction): Promise<void> {
		if (action.lesson_id === null) return;
		restoringActionId = action.id;
		const nextErrors = { ...restoreErrors };
		delete nextErrors[action.id];
		restoreErrors = nextErrors;
		try {
			await restoreLesson(action.lesson_id);
			restoredActionIds = new Set(restoredActionIds).add(action.id);
			restoreAnnouncement = `Lesson #${action.lesson_id} restored.`;
		} catch (err) {
			restoreErrors = {
				...restoreErrors,
				[action.id]: err instanceof ApiError ? err.message : String(err)
			};
			restoreAnnouncement = `Couldn't restore lesson #${action.lesson_id}.`;
		} finally {
			restoringActionId = null;
		}
	}
</script>

<section
	class="sma-card"
	class:sma-card--grouped={grouped}
	data-design="funk"
	data-augmented-ui={grouped ? undefined : 'tl-clip br-clip border'}
	aria-labelledby={headingId}
>
	<h3 class="sma-heading">
		<div class="band-hd-wrap">
			<button
				type="button"
				class="band-hd"
				onclick={toggleSection}
				aria-expanded={sectionExpanded}
				aria-controls="sma-body"
			>
				<DisclosureMarker open={sectionExpanded} size={13} />
				<History size={13} aria-hidden="true" />
				<span id={headingId} class="panel-hd">Memory actions</span>
				{#if actions.length > 0}
					<span class="band-hd__meta">{actions.length}</span>
				{/if}
			</button>
			<Tooltip
				text="What memory consolidation did with lessons recorded from this session — promotions, archives, and restores, with the evidence behind each."
				placement="bottom"
			>
				<Info size={13} aria-hidden="true" class="band-hd__help" />
			</Tooltip>
		</div>
	</h3>

	<span class="sr-only" role="status" aria-live="polite">{restoreAnnouncement}</span>

	{#if sectionExpanded}
		<div id="sma-body" class="sma-body band-body">
			{#if loading && actions.length === 0}
				<div class="sma-skeleton" aria-hidden="true">
					<div class="sma-skel-row"></div>
					<div class="sma-skel-row sma-skel-row--short"></div>
				</div>
			{:else if error}
				<LoadErrorBanner
					title="Couldn't load memory actions"
					detail={error}
					hint="The session memory-actions endpoint may not be reachable yet."
					onRetry={retry}
				/>
			{:else if actions.length === 0}
				<p class="sma-empty">No memory actions recorded for this session yet.</p>
			{:else}
				<ul class="sma-list" role="list">
					{#each actions as action (action.id)}
						<li>
							<DecisionActionCard
								{action}
								onRestore={handleRestore}
								restoring={restoringActionId === action.id}
								restoreError={restoreErrors[action.id] ?? null}
								restored={restoredActionIds.has(action.id)}
							/>
						</li>
					{/each}
				</ul>
			{/if}
		</div>
	{/if}
</section>

<style>
	.sma-card {
		--edge: var(--edge-sm);
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: var(--hairline);
		--aug-border-bg: var(--edge-line);
		background: var(--edge-surface);
		display: flex;
		flex-direction: column;
	}

	.sma-card--grouped {
		background: transparent;
	}

	.sma-heading {
		display: contents;
	}

	.sma-list {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.sma-empty {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
		font-style: italic;
	}

	.sma-skeleton {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.sma-skel-row {
		height: 14px;
		border-radius: var(--r-tol);
		background: var(--surface-2);
	}
	.sma-skel-row--short {
		width: 60%;
	}
</style>
