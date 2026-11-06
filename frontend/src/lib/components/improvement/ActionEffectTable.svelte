<!--
@component
What each action in force has done, in one frame, grouped into fixed-order bands: Fewer
corrections, No change seen, Guard questions (checked against held-back turns), and Not enough
data yet (closed at first). Each band opens and closes in place; each row is an ActionEffectRow.
-->
<script lang="ts">
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import { disclose } from '$lib/components/ui/motion';
	import ActionEffectRow from './ActionEffectRow.svelte';
	import type { ActionEffectItem } from '$lib/api/improvement';

	interface Props {
		actions: ActionEffectItem[];
		error: string | null;
		onRetry: () => void;
	}

	const { actions, error, onRetry }: Props = $props();

	type GroupKey = 'fewer' | 'no_change' | 'randomised' | 'not_enough' | 'not_computed';

	const GROUPS: { key: GroupKey; label: string; open: boolean }[] = [
		{ key: 'fewer', label: 'Fewer corrections', open: true },
		{ key: 'no_change', label: 'No change seen', open: true },
		{ key: 'randomised', label: 'Guard questions, checked against held-back turns', open: true },
		{ key: 'not_enough', label: 'Not enough data yet', open: false },
		{ key: 'not_computed', label: 'Not computed yet', open: true }
	];

	function groupOf(action: ActionEffectItem): GroupKey {
		const effect = action.effect;
		if (effect.status === 'not_computed') return 'not_computed';
		if (effect.status === 'randomised') return 'randomised';
		if (effect.status === 'not_enough_data') return 'not_enough';
		return effect.verdict === 'fewer_corrections' ? 'fewer' : 'no_change';
	}

	function byDecidedDesc(a: ActionEffectItem, b: ActionEffectItem): number {
		if (a.decidedAt !== b.decidedAt) {
			if (a.decidedAt === null) return 1;
			if (b.decidedAt === null) return -1;
			return b.decidedAt.localeCompare(a.decidedAt);
		}
		return String(a.id).localeCompare(String(b.id), undefined, { numeric: true });
	}

	const grouped = $derived(
		GROUPS.map((g) => ({
			...g,
			members: actions.filter((a) => groupOf(a) === g.key).sort(byDecidedDesc)
		})).filter((g) => g.members.length > 0)
	);

	let openGroups = $state<Record<GroupKey, boolean>>(
		Object.fromEntries(GROUPS.map((g) => [g.key, g.open])) as Record<GroupKey, boolean>
	);
</script>

<section
	class="aet edge mech-group"
	aria-label="What each action has done"
	data-slot="action-effect-table"
>
	<div class="band-hd band-hd--static">
		<h3 class="machine aet-title">What each action has done · {actions.length}</h3>
	</div>
	{#if error}
		<div class="band-body">
			<LoadErrorBanner title="Could not load action effects." detail={error} {onRetry} />
		</div>
	{:else if actions.length === 0}
		<div class="band-body">
			<p class="aet-empty">No lessons, directives or guard questions are active yet.</p>
		</div>
	{:else}
		{#each grouped as group (group.key)}
			<div class="aet-group" data-group={group.key}>
				<div class="band-hd-wrap">
					<button
						type="button"
						class="band-hd"
						aria-expanded={openGroups[group.key]}
						aria-controls={`aet-${group.key}`}
						onclick={() => (openGroups[group.key] = !openGroups[group.key])}
					>
						<DisclosureMarker open={openGroups[group.key]} size={10} />
						<span class="machine aet-label">{group.label}</span>
						<span class="band-hd__meta">{group.members.length}</span>
					</button>
				</div>
				{#if openGroups[group.key]}
					<div class="aet-body" id={`aet-${group.key}`} transition:disclose>
						<div class="aet-rows">
							{#each group.members as action (action.kind + ':' + action.id)}
								<ActionEffectRow {action} />
							{/each}
						</div>
					</div>
				{/if}
			</div>
		{/each}
	{/if}
</section>

<style>
	.aet {
		--edge: var(--edge-sm);
		min-width: 0;
	}
	.aet-title {
		margin: 0;
		font-size: var(--text-xs);
		font-weight: var(--weight-body);
		color: var(--text-1);
	}
	.aet-label {
		font-size: var(--text-xs);
		color: var(--text-1);
	}
	.aet-empty {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.aet-body {
		display: grid;
	}
	.aet-rows {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		min-height: 0;
		overflow: hidden;
	}
</style>
