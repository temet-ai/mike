<script lang="ts">
	import { CircleAlert, Scale } from '@lucide/svelte';
	import type { WrightAction, WrightProblem } from '$lib/api/wright';
	import Badge from '$lib/components/shared/Badge.svelte';
	import type { BadgeVariant } from '$lib/components/shared/Badge.variants';
	import LinkChipRow, { type LinkChip } from '$lib/components/shared/LinkChipRow.svelte';
	import type { UndoStatus } from '$lib/wright/store.svelte';
	import {
		actionMeasure,
		actionStateBadge,
		clockTime,
		dayAndTime,
		remedyTitle
	} from '$lib/wright/present';
	import UndoSlot, { type UndoState } from './UndoSlot.svelte';
	import WrightFacts from './WrightFacts.svelte';

	interface Props {
		action: WrightAction;
		/** The action's problem when it is on screen: names it and says whether its measure moved. */
		problem?: WrightProblem;
		undo?: UndoStatus;
		/** Why Undo is off for this browser, or null when it can be used. */
		undoDisabledReason?: string | null;
		showDay?: boolean;
		onUndo: (id: number) => void;
	}

	const {
		action,
		problem,
		undo,
		undoDisabledReason = null,
		showDay = false,
		onUndo
	}: Props = $props();

	const TONE: Record<string, BadgeVariant> = {
		active: 'chip-active',
		muted: 'chip-muted',
		warning: 'chip-warning',
		error: 'chip-error',
		analyzed: 'chip-analyzed'
	};

	const badge = $derived(actionStateBadge(action.state));
	const measure = $derived(actionMeasure(action, problem));
	const undone = $derived(action.state === 'undone');
	const slot = $derived<UndoState>(
		undone
			? 'undone'
			: undo?.state === 'undoing'
				? 'undoing'
				: undo?.state === 'failed'
					? 'failed'
					: action.can_undo
						? 'ready'
						: 'none'
	);
	const blocked = $derived(
		undo?.state === 'failed'
			? undo.reason
			: action.state === 'undo_conflict'
				? 'The resource changed after this action, so it was not undone.'
				: null
	);
	const links = $derived<LinkChip[]>([
		{
			icon: CircleAlert,
			kind: 'Problem',
			text: `${action.problem_id}`,
			href: `#problem-${action.problem_id}`
		},
		...(measure === null
			? []
			: [
					{
						icon: Scale,
						kind: 'Measure',
						text:
							action.state === 'ineffective'
								? 'Did not move'
								: problem?.state === 'healthy'
									? 'Healthy again'
									: 'Checked'
					}
				])
	]);
</script>

<li
	class="action"
	class:action--undone={undone}
	id="action-{action.id}"
	data-slot="wright-action-row"
>
	<span class="action__time"
		>{showDay ? dayAndTime(action.created_at) : clockTime(action.created_at)}</span
	>
	<div class="action__main">
		<p class="action__text">
			{remedyTitle(action.remedy)}
			{#if !undone}<Badge variant={TONE[badge.tone] ?? 'chip-muted'}>{badge.label}</Badge>{/if}
		</p>
		<p class="action__meta">
			By {action.by === 'wright' ? 'Wright' : 'you'}
			{#if action.verified_at}· verified {dayAndTime(action.verified_at)}{/if}
		</p>
		{#if measure}<p class="action__meta">{measure}</p>{/if}
		<WrightFacts facts={action.effect} label="What the action changed" />
		{#if blocked}<p class="action__blocked" role="alert">Couldn't undo: {blocked}</p>{/if}
		<LinkChipRow items={links} label="Evidence" />
	</div>
	<UndoSlot
		subject="{remedyTitle(action.remedy)}, action {action.id}"
		state={slot}
		reason={blocked}
		disabledReason={undoDisabledReason}
		onUndo={() => onUndo(action.id)}
	/>
</li>

<style>
	.action {
		display: flex;
		align-items: flex-start;
		gap: var(--space-3);
		padding: var(--space-3) var(--space-2);
		scroll-margin-top: var(--space-6);
	}

	.action:not(:first-child) {
		border-top: var(--hairline) solid var(--border-subtle);
	}

	.action:target {
		background: var(--surface-2);
	}

	.action--undone .action__text {
		color: var(--text-3);
	}

	.action__time {
		flex: none;
		inline-size: 3rem;
		padding-top: 2px;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-3);
	}

	.action__main {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.action__text {
		margin: 0;
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		line-height: 1.5;
		color: var(--text-1);
		overflow-wrap: anywhere;
	}

	.action__meta {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-3);
	}

	.action__blocked {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--status-warning-fg);
		overflow-wrap: anywhere;
	}

	@container (max-width: 860px) {
		.action {
			flex-wrap: wrap;
			row-gap: var(--space-2);
		}

		.action__main {
			flex-basis: calc(100% - 3rem - var(--space-3));
		}
	}
</style>
