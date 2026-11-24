<script lang="ts">
	import {
		ArrowUpDown,
		CirclePlus,
		Folder,
		FolderMinus,
		Gauge,
		GitMerge,
		Link,
		PauseCircle,
		Pin,
		Play,
		SkipForward,
		Split,
		Wrench
	} from '@lucide/svelte';
	import type { Component } from 'svelte';
	import type { SteerChange } from '$lib/api/wright';
	import LinkChipRow, { type LinkChip } from '$lib/components/shared/LinkChipRow.svelte';
	import { describeSteer, type ChangeIcon } from '$lib/wright/present';
	import UndoSlot, { type UndoState } from './UndoSlot.svelte';

	interface Props {
		/** The steer Wright applied, as the closed set of changes it may propose. */
		change: SteerChange;
		undo: UndoState;
		undoDisabledReason?: string | null;
		undoReason?: string | null;
		onUndo: () => void;
	}

	const { change, undo, undoDisabledReason = null, undoReason = null, onUndo }: Props = $props();

	const ICON: Record<ChangeIcon, Component> = {
		pause: PauseCircle,
		resume: Play,
		open: CirclePlus,
		limit: Gauge,
		skip: SkipForward,
		pin: Pin,
		order: ArrowUpDown,
		merge: GitMerge,
		separate: Split,
		group: Folder,
		ungroup: FolderMinus,
		other: Wrench
	};

	const view = $derived(describeSteer(change));
	const Icon = $derived(ICON[view.icon]);
	const links = $derived<LinkChip[]>(view.links.map((l) => ({ icon: Link, ...l })));
</script>

<div class="edge change" class:change--undone={undo === 'undone'} data-slot="wright-change">
	<div class="change__row">
		<Icon size={16} aria-hidden="true" class="change__icon" />
		<div class="change__text">
			<p class="change__title">{view.title}</p>
			<p class="change__detail">{view.detail}</p>
			<LinkChipRow items={links} label="Causes named by this change" />
		</div>
		<UndoSlot
			subject={view.title}
			state={undo}
			reason={undoReason}
			disabledReason={undoDisabledReason}
			{onUndo}
		/>
	</div>
	{#if undo === 'failed' && undoReason}
		<p class="change__blocked" role="alert">Couldn't undo: {undoReason}</p>
	{/if}
</div>

<style>
	.change {
		--edge: var(--edge-sm);
		--edge-line: var(--border-default);
		--edge-surface: var(--surface-2);
	}

	.change__row {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.change__row :global(.change__icon) {
		flex: none;
		color: var(--signal-active);
	}

	.change--undone .change__row :global(.change__icon),
	.change--undone .change__title {
		color: var(--text-3);
	}

	.change__text {
		flex: 1;
		min-width: 0;
	}

	.change__title {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		font-weight: 600;
		color: var(--text-strong);
		overflow-wrap: anywhere;
	}

	.change__detail,
	.change__blocked {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-3);
		overflow-wrap: anywhere;
	}

	.change__blocked {
		margin-top: var(--space-1);
		color: var(--status-warning-fg);
	}

	@container (max-width: 860px) {
		.change__row {
			flex-wrap: wrap;
		}

		.change__text {
			flex-basis: calc(100% - 16px - var(--space-2));
		}
	}
</style>
