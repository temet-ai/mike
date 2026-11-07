<!--
@component
Every open standing-directive proposal, one `DirectiveProposalCard` band each.

Two modes, same markup: pass `proposals` (already resolved by a page's own `load()`, e.g.
`/sessions/improvement`'s "Waiting for you" grid) and this component renders it as-is with no
fetch of its own — `error`/`emptyText` behave exactly like `GuardWakeList`'s (§4.6): `emptyText`
unset means an empty result renders nothing (the grid handles "nothing waiting" itself);
`emptyText` set means it renders that text instead. Omit `proposals` (the `/memory` page's own
usage) and the component fetches and owns its own loading/empty state, `emptyText` defaulting
to "No open proposals." — this is the one context where an onMount fetch is fine, since `/memory`
has no page-level `load()` for this data and no layout-shift contract to keep.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { listProposals } from '$lib/api/directiveProposals';
	import type { DirectiveProposal } from '$lib/api/directiveProposals';
	import DirectiveProposalCard from './DirectiveProposalCard.svelte';

	interface Props {
		proposals?: DirectiveProposal[];
		error?: string | null;
		emptyText?: string;
		onDecided?: () => void;
	}

	let {
		proposals: controlledProposals,
		error: controlledError,
		emptyText,
		onDecided
	}: Props = $props();

	const controlled = $derived(controlledProposals !== undefined);

	let fetchedProposals = $state<DirectiveProposal[]>([]);
	let fetchError = $state<string | null>(null);

	const proposals = $derived(controlled ? (controlledProposals ?? []) : fetchedProposals);
	const loadError = $derived(controlled ? (controlledError ?? null) : fetchError);
	const resolvedEmptyText = $derived(emptyText ?? (controlled ? undefined : 'No open proposals.'));

	async function load(): Promise<void> {
		try {
			fetchedProposals = (await listProposals('open')).proposals ?? [];
			fetchError = null;
		} catch (e) {
			fetchError = e instanceof Error ? e.message : String(e);
		}
	}

	function decided(): void {
		if (controlled) {
			onDecided?.();
		} else {
			void load();
		}
	}

	onMount(() => {
		if (!controlled) void load();
	});

	const showsNothing = $derived(
		proposals.length === 0 && !loadError && resolvedEmptyText === undefined
	);
</script>

{#if !showsNothing}
	<section class="dps edge mech-group" data-testid="directive-proposals">
		<div class="band-hd band-hd--static">
			<h2 class="machine dps-title">
				Proposed standing directives{proposals.length > 0 ? ` · ${proposals.length}` : ''}
			</h2>
		</div>
		<div class="band-body dps-body">
			<p class="dps-note">
				Instructions you had to give more than once, and steps you confirmed were already
				authorised.
			</p>
			{#if loadError}
				<p class="dps-error" role="alert">Could not load proposals: {loadError}</p>
			{:else if proposals.length === 0}
				<p class="dps-empty">{resolvedEmptyText}</p>
			{/if}
		</div>
		{#if !loadError}
			{#each proposals as p (p.id)}
				<DirectiveProposalCard proposal={p} onDecided={decided} />
			{/each}
		{/if}
	</section>
{/if}

<style>
	.dps {
		--edge: var(--edge-sm);
		min-width: 0;
	}
	.dps-title {
		margin: 0;
		font-size: var(--text-xs);
		font-weight: var(--weight-body);
		color: var(--text-1);
	}
	.dps-body {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.dps-note {
		margin: 0;
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.dps-empty {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.dps-error {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--status-danger-fg);
	}
</style>
