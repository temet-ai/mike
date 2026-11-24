<!--
@component
One cause in the Triage queue: the state Wright has reached on it, the check, how many open problems
share it, the resource, how long it has been open and how many times the engineer has looked.
A line under the resource says where the cause belongs: its area, its group, whether it is pinned,
whether the operator moved it and how many causes were merged into it. Clicking the row opens it.
The operator's actions (pin, move up and down, reset, group, a checkbox to select) sit beside the
toggle, never inside it, and keep their slots whether or not they apply, so a state change repaints
in place. A waiting cause carries the warning border; the border is always reserved.
-->
<script lang="ts">
	import {
		ArrowDown,
		ArrowUp,
		ArrowUpDown,
		CircleDot,
		FileSearch,
		FolderPlus,
		Folder,
		GitMerge,
		GripVertical,
		Layers,
		Link,
		ListOrdered,
		Pin,
		PinOff,
		RotateCcw,
		TriangleAlert,
		Wrench
	} from '@lucide/svelte';
	import type { Component, Snippet } from 'svelte';
	import type { WrightCause } from '$lib/api/wright';
	import Badge from '$lib/components/shared/Badge.svelte';
	import LinkChipRow, { type LinkChip } from '$lib/components/shared/LinkChipRow.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Checkbox } from '$lib/components/ui/checkbox';
	import {
		attemptsText,
		causeArea,
		causeBadge,
		causeLinks,
		causeState,
		causeTooltip,
		isMoved,
		openFor,
		problemsText,
		type CauseState,
		type DropEdge,
		type MoveDirection
	} from '$lib/wright/causes';
	import { dayAndTime } from '$lib/wright/present';

	interface Props {
		cause: WrightCause;
		/** A cause nothing has answered for this long is "waiting": the one the operator must see. */
		waitedHours: number;
		/** Why the operator's actions are off (this browser is not paired); null when they are on. */
		disabledReason?: string | null;
		/** An action is on its way: the actions wait for it. */
		busy?: boolean;
		/** With `onSelect`, the row has a checkbox to select it for Merge into and Group. */
		selected?: boolean;
		onSelect?: (selected: boolean) => void;
		/** Whether the cause has a neighbour to pass, within its pin state. */
		canMoveUp?: boolean;
		canMoveDown?: boolean;
		onPin?: (pinned: boolean) => void;
		onMove?: (direction: MoveDirection) => void;
		onReset?: () => void;
		onGroup?: () => void;
		/** Ranked view: the row can be dragged by its grip. Move up and down do the same. */
		draggable?: boolean;
		/** Where a dragged cause would land on this row. */
		dropEdge?: DropEdge | null;
		onDragStart?: (id: number) => void;
		onDragOver?: (id: number, edge: DropEdge) => void;
		onDrop?: (id: number, edge: DropEdge) => void;
		onDragEnd?: () => void;
		/** Called the first time the detail opens, so the page can load the cause's problems. */
		onOpen?: (cause: WrightCause) => void;
		/** The detail below the resource: score, engineer, problems, merged causes. */
		children?: Snippet;
	}

	const {
		cause,
		waitedHours,
		disabledReason = null,
		busy = false,
		selected = false,
		onSelect,
		canMoveUp = false,
		canMoveDown = false,
		onPin,
		onMove,
		onReset,
		onGroup,
		draggable = false,
		dropEdge = null,
		onDragStart,
		onDragOver,
		onDrop,
		onDragEnd,
		onOpen,
		children
	}: Props = $props();

	const GLYPH: Record<CauseState, Component> = {
		found: FileSearch,
		filed: CircleDot,
		known_fix: Wrench,
		waiting: TriangleAlert,
		queued: ListOrdered
	};

	let open = $state(false);
	let opened = false;
	$effect(() => {
		if (open && !opened) {
			opened = true;
			onOpen?.(cause);
		}
	});

	const reached = $derived(causeState(cause, waitedHours));
	const badge = $derived(causeBadge(reached));
	const Glyph = $derived(GLYPH[reached]);
	const waiting = $derived(reached === 'waiting');
	const area = $derived(causeArea(cause));
	const moved = $derived(isMoved(cause));
	const off = $derived(disabledReason !== null || busy);
	/** "16 problems · Open 30h · no attempts": the count is left out for a cause of one problem. */
	const metaLine = $derived(
		[
			...(cause.problems > 1 ? [problemsText(cause.problems)] : []),
			openFor(cause.opened_at),
			attemptsText(cause.attempts)
		].join(' · ')
	);

	const links = $derived<LinkChip[]>(
		causeLinks(cause).map((link) => ({
			icon: link.kind === 'finding' ? FileSearch : link.kind === 'issue' ? CircleDot : Link,
			kind: link.kind === 'finding' ? 'Finding' : link.kind === 'issue' ? 'Issue' : 'Problem',
			text: link.text,
			href: link.href,
			external: link.external
		}))
	);

	const stands = $derived(
		[
			['Opened', cause.opened_at],
			['Last attempt', cause.last_attempt_at],
			['Operator told', cause.notified_at],
			['Wright commented', cause.commented_at]
		].filter((line): line is [string, string] => line[1] !== null)
	);

	let dragging = $state(false);

	function edgeAt(event: DragEvent, row: HTMLElement): DropEdge {
		const box = row.getBoundingClientRect();
		return event.clientY < box.top + box.height / 2 ? 'before' : 'after';
	}

	function startDrag(event: DragEvent): void {
		if (event.dataTransfer !== null) {
			event.dataTransfer.effectAllowed = 'move';
			event.dataTransfer.setData('text/plain', String(cause.cause_id));
		}
		dragging = true;
		onDragStart?.(cause.cause_id);
	}

	function endDrag(): void {
		dragging = false;
		onDragEnd?.();
	}

	function overRow(event: DragEvent): void {
		if (onDragOver === undefined) return;
		event.preventDefault();
		onDragOver(cause.cause_id, edgeAt(event, event.currentTarget as HTMLElement));
	}

	function dropOnRow(event: DragEvent): void {
		if (onDrop === undefined) return;
		event.preventDefault();
		onDrop(cause.cause_id, edgeAt(event, event.currentTarget as HTMLElement));
	}
</script>

<!--
	Drag and drop is one more way to do what Move up and Move down already do from the keyboard,
	so the row itself is not given a role.
-->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<li
	class="cause"
	class:cause--waiting={waiting}
	class:cause--selected={selected}
	class:cause--dragging={dragging}
	id="cause-{cause.cause_id}"
	data-slot="wright-cause-row"
	data-state={reached}
	data-pinned={cause.pinned ? '' : undefined}
	data-drop={dropEdge ?? undefined}
	ondragover={overRow}
	ondrop={dropOnRow}
>
	<div class="cause__main">
		<span class="cause__select">
			{#if onSelect}
				<Checkbox
					checked={selected}
					onchange={(event) => onSelect(event.currentTarget.checked)}
				>
					{#snippet label()}
						<span class="sr-only">Select {cause.check_name}</span>
					{/snippet}
				</Checkbox>
			{/if}
		</span>
		<span
			class="cause__grip"
			class:cause__grip--on={draggable && !off}
			{draggable}
			ondragstart={startDrag}
			ondragend={endDrag}
			aria-hidden="true"
		>
			<GripVertical size={14} aria-hidden="true" />
		</span>
		<button
			type="button"
			class="cause__toggle"
			aria-expanded={open}
			aria-controls="cause-{cause.cause_id}-detail"
			onclick={() => (open = !open)}
		>
			<span class="cause__head">
				<span class="cause__title">{cause.check_name}</span>
				<span class="cause__badge">
					<Tooltip text={causeTooltip(reached, waitedHours)}>
						{#snippet child({ props })}
							<span {...props}>
								<Badge variant={badge.variant}>
									<span class="cause__state">
										<Glyph size={11} aria-hidden="true" />
										{badge.label}
									</span>
								</Badge>
							</span>
						{/snippet}
					</Tooltip>
				</span>
			</span>
			<span class="cause__resource" title={cause.resource}>{cause.resource}</span>
			<span class="cause__tags">
				{#if cause.pinned}
					<span class="cause__tag cause__tag--pin"><Pin size={11} aria-hidden="true" />Pinned</span>
				{/if}
				<span class="cause__tag"><Layers size={11} aria-hidden="true" />{area.label}</span>
				{#if cause.group_name !== null}
					<span class="cause__tag"><Folder size={11} aria-hidden="true" />{cause.group_name}</span>
				{/if}
				{#if moved}
					<span class="cause__tag">
						<ArrowUpDown size={11} aria-hidden="true" />Moved by you
					</span>
				{/if}
				{#if cause.absorbed.length > 0}
					<span class="cause__tag">
						<GitMerge size={11} aria-hidden="true" />{cause.absorbed.length} merged
					</span>
				{/if}
			</span>
			<span class="cause__meta" class:cause__meta--waiting={waiting}>
				{metaLine}
			</span>
		</button>
		<div class="cause__actions" role="group" aria-label="Actions for {cause.check_name}">
			<Button
				variant="ghost"
				size="icon"
				class="cause__act"
				aria-label="{cause.pinned ? 'Unpin' : 'Pin'} {cause.check_name}"
				title={disabledReason ?? (cause.pinned ? 'Unpin' : 'Pin')}
				disabled={off || onPin === undefined}
				onclick={() => onPin?.(!cause.pinned)}
			>
				{#if cause.pinned}<PinOff aria-hidden="true" />{:else}<Pin aria-hidden="true" />{/if}
			</Button>
			<Button
				variant="ghost"
				size="icon"
				class="cause__act"
				aria-label="Move {cause.check_name} up"
				title={disabledReason ?? 'Move up'}
				disabled={off || !canMoveUp || onMove === undefined}
				onclick={() => onMove?.('up')}
			>
				<ArrowUp aria-hidden="true" />
			</Button>
			<Button
				variant="ghost"
				size="icon"
				class="cause__act"
				aria-label="Move {cause.check_name} down"
				title={disabledReason ?? 'Move down'}
				disabled={off || !canMoveDown || onMove === undefined}
				onclick={() => onMove?.('down')}
			>
				<ArrowDown aria-hidden="true" />
			</Button>
			<Button
				variant="ghost"
				size="icon"
				class="cause__act {moved ? '' : 'cause__act--idle'}"
				aria-label="Reset the order of {cause.check_name}"
				title={disabledReason ?? 'Reset order'}
				disabled={off || !moved || onReset === undefined}
				onclick={() => onReset?.()}
			>
				<RotateCcw aria-hidden="true" />
			</Button>
			<Button
				variant="ghost"
				size="icon"
				class="cause__act"
				aria-label="Add {cause.check_name} to a group"
				title={disabledReason ?? 'Add to group'}
				disabled={off || onGroup === undefined}
				onclick={() => onGroup?.()}
			>
				<FolderPlus aria-hidden="true" />
			</Button>
		</div>
	</div>
	<div class="cause__links">
		<LinkChipRow items={links} label="Links for cause {cause.cause_id}" />
	</div>
	{#if open}
		<div class="cause__detail" id="cause-{cause.cause_id}-detail">
			<div class="cause__section">
				<h3 class="machine cause__h">Resource</h3>
				<p class="cause__full">{cause.resource}</p>
			</div>
			<div class="cause__section">
				<h3 class="machine cause__h">Where it stands</h3>
				<p class="cause__stand">
					{problemsText(cause.problems)} open · {cause.observations} observations · {attemptsText(
						cause.attempts
					)}
				</p>
				{#each stands as [label, at] (label)}
					<p class="cause__stand">{label} {dayAndTime(at)}</p>
				{/each}
			</div>
			{@render children?.()}
		</div>
	{/if}
</li>

<style>
	.cause {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding: var(--space-3) var(--space-2);
		scroll-margin-top: var(--space-6);
	}

	.cause:not(:first-child) {
		border-top: var(--hairline) solid var(--border-subtle);
	}

	/* An inset ring takes no space: a row turning waiting repaints without moving its neighbours. */
	.cause--waiting {
		box-shadow: inset 0 0 0 var(--hairline) var(--status-warning-border);
	}

	.cause--selected {
		background: var(--interactive-active-bg);
	}

	.cause--dragging {
		opacity: 0.5;
	}

	/* Where a dragged cause would land: a line inside the row's own box, so nothing moves. */
	.cause[data-drop='before'] {
		box-shadow: inset 0 2px 0 var(--interactive-active-border);
	}

	.cause[data-drop='after'] {
		box-shadow: inset 0 -2px 0 var(--interactive-active-border);
	}

	/* Checkbox, grip, the toggle and the actions each keep their column in every state. */
	.cause__main {
		display: grid;
		grid-template-columns: 1.25rem 1rem minmax(0, 1fr) auto;
		column-gap: var(--space-2);
		align-items: start;
	}

	.cause__select {
		display: flex;
		align-items: center;
		min-block-size: var(--space-5);
	}

	.cause__grip {
		display: flex;
		align-items: center;
		justify-content: center;
		min-block-size: var(--space-5);
		color: var(--text-3);
		visibility: hidden;
	}

	.cause__grip--on {
		visibility: visible;
		cursor: grab;
	}

	.cause__toggle {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		min-width: 0;
		padding: 0;
		border: 0;
		background: transparent;
		color: inherit;
		font: inherit;
		text-align: left;
		cursor: pointer;
	}

	.cause__toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.cause__head {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
	}

	.cause__title {
		flex: 1;
		min-width: 0;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		font-weight: 600;
		color: var(--text-strong);
		overflow-wrap: anywhere;
	}

	.cause__badge {
		flex: none;
		order: 9;
		margin-left: auto;
	}

	.cause__state {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
	}

	/* One line: the whole text is in the title and in the opened detail. */
	.cause__resource {
		min-width: 0;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		line-height: 1.5;
		color: var(--text-1);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.cause__tags {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-1) var(--space-3);
		min-block-size: 1.25rem;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-2);
	}

	.cause__tag {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		min-width: 0;
		overflow-wrap: anywhere;
	}

	.cause__tag--pin {
		color: var(--interactive-active-fg);
	}

	.cause__meta {
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-3);
	}

	.cause__meta--waiting {
		color: var(--status-warning-fg);
	}

	.cause__actions {
		display: flex;
		gap: 2px;
	}

	.cause__actions :global(.cause__act) {
		inline-size: 1.75rem;
		block-size: 1.75rem;
	}

	/* Reset keeps its slot when there is nothing to reset. */
	.cause__actions :global(.cause__act--idle) {
		visibility: hidden;
	}

	.cause__links {
		/* Reserved for one row of chips: a state that adds a link does not move the rows below. */
		min-block-size: 1.5rem;
		padding-inline-start: calc(1.25rem + 1rem + var(--space-2) * 2);
	}

	.cause__detail {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding-top: var(--space-2);
		padding-inline-start: calc(1.25rem + 1rem + var(--space-2) * 2);
	}

	.cause__section {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.cause__h {
		margin: 0;
		font-size: var(--text-micro);
		color: var(--text-3);
	}

	.cause__full {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		line-height: 1.5;
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}

	.cause__stand {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-3);
		overflow-wrap: anywhere;
	}

	@media (max-width: 640px) {
		/* The actions drop under the text at phone width; the row keeps that shape in every state. */
		.cause__actions {
			grid-column: 3 / -1;
			grid-row: 2;
			padding-block-start: var(--space-1);
		}
	}
</style>
