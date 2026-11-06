<!--
@component
Delivered guard nudges in one frame, one `GuardWakeCard` band each, newest first. A lead line says
what marking a nudge right or wrong does (drop it with `lead={false}` where the page states it
already). Nudges you can answer come first; nudges whose next turn has not been judged follow
under "Not ready to answer yet". `title` adds a header band with the count; the landing page's
Nudges tab names the list itself, so it passes none. `emptyText` set means an empty list says so.
-->
<script lang="ts">
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import GuardWakeCard from './GuardWakeCard.svelte';
	import ProjectConversationGroups from './ProjectConversationGroups.svelte';
	import { isJudged } from '$lib/api/guardEffect';
	import type { GuardWake } from '$lib/api/guardEffect';

	interface Props {
		title?: string;
		wakes: GuardWake[];
		error: string | null;
		emptyText?: string;
		lead?: boolean;
		onLabel: (evaluationUuid: string, label: 'right' | 'wrong') => Promise<void>;
		onRetry: () => void;
	}

	let { title, wakes, error, emptyText, lead = true, onLabel, onRetry }: Props = $props();

	let busyUuid = $state<string | null>(null);

	async function label(evaluationUuid: string, value: 'right' | 'wrong'): Promise<void> {
		busyUuid = evaluationUuid;
		try {
			await onLabel(evaluationUuid, value);
		} finally {
			busyUuid = null;
		}
	}

	const newestFirst = $derived(
		[...wakes].sort((a, b) => b.deliveredAt.localeCompare(a.deliveredAt))
	);
	const answerable = $derived(newestFirst.filter(isJudged));
	const notJudged = $derived(newestFirst.filter((w) => !isJudged(w)));
	const showsNothing = $derived(wakes.length === 0 && !error && !emptyText && !lead && !title);
</script>

{#snippet card(wake: GuardWake)}
	<GuardWakeCard
		{wake}
		showSession={false}
		busy={busyUuid === wake.evaluationUuid}
		onLabel={(l) => label(wake.evaluationUuid, l)}
	/>
{/snippet}

{#snippet grouped(list: GuardWake[])}
	<ProjectConversationGroups
		items={list}
		key={(w) => w.evaluationUuid}
		one="nudge"
		many="nudges"
		item={card}
	/>
{/snippet}

{#if !showsNothing}
	<div class="gwl edge mech-group" data-testid="guard-wake-list">
		{#if title}
			<div class="band-hd band-hd--static">
				<h2 class="machine gwl-title">{title}{wakes.length > 0 ? ` · ${wakes.length}` : ''}</h2>
			</div>
		{/if}
		{#if lead}
			<div class="band-body gwl-lead" data-testid="guard-marks-effect">
				Marking a nudge right or wrong tells its question how often it is right. If too many are
				wrong, the question goes back to being tried quietly.
			</div>
		{/if}
		{#if error}
			<div class="band-body">
				<LoadErrorBanner title="Could not load delivered wakes." detail={error} {onRetry} />
			</div>
		{:else if wakes.length === 0}
			{#if emptyText}<div class="band-body gwl-empty">{emptyText}</div>{/if}
		{:else}
			{@render grouped(answerable)}
			{#if notJudged.length > 0}
				<div class="band-hd band-hd--static">
					<h3 class="machine gwl-title">Not ready to answer yet</h3>
					<span class="band-hd__meta">{notJudged.length} — the next turn has not been judged</span>
				</div>
				{@render grouped(notJudged)}
			{/if}
		{/if}
	</div>
{/if}

<style>
	.gwl {
		--edge: var(--edge-sm);
		min-width: 0;
	}
	.gwl-title {
		margin: 0;
		font-size: var(--text-xs);
		font-weight: var(--weight-body);
		color: var(--text-1);
	}
	.gwl-lead,
	.gwl-empty {
		padding-top: var(--space-3);
		font-size: var(--text-sm);
		color: var(--text-2);
	}
</style>
