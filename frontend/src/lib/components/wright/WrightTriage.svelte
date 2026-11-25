<!--
@component
The triage band's body: the queue of causes in the order the engineer works in, switchable to the By
area view (a section for each named group, then each derived area), and what the operator can do to
it. Rows are selected with a checkbox for Merge into and Group; each row can be pinned, moved up or
down (or dragged on the Ranked view) and added to a group. Unpaired, every action is off and the one
line under the switch says why. The list and the two views keep the order they are given: the store
holds it still while Wright refreshes and reorders it after an action.
-->
<script module lang="ts">
	export type TriageView = 'ranked' | 'area';
</script>

<script lang="ts">
	import { Folder, GitMerge, FolderPlus, X } from '@lucide/svelte';
	import { untrack, type Snippet } from 'svelte';
	import type { WrightCause, WrightGroup, WrightProblem } from '$lib/api/wright';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import { Button } from '$lib/components/ui/button';
	import * as Tabs from '$lib/components/ui/tabs';
	import {
		canMove,
		dropStep,
		moveStep,
		problemsText,
		sectionsByArea,
		type DropEdge,
		type MoveDirection
	} from '$lib/wright/causes';
	import type { Feed, FeedStatus } from '$lib/wright/feed';
	import { PAIR_COMMAND } from '$lib/wright/pairing';
	import type { GroupTarget } from '$lib/wright/store.svelte';
	import WrightCauseDetail from './WrightCauseDetail.svelte';
	import WrightCauseRow from './WrightCauseRow.svelte';
	import WrightGroupDialog from './WrightGroupDialog.svelte';
	import WrightMergeDialog from './WrightMergeDialog.svelte';
	import WrightNote from './WrightNote.svelte';

	interface Props {
		causes: WrightCause[];
		status: FeedStatus;
		error?: string | null;
		waitedHours: number;
		groups: WrightGroup[];
		maxAttempts: number | null;
		/** The open problems of the causes whose rows were opened, by cause id. */
		causeProblems: Record<number, Feed<WrightProblem>>;
		/** True when this browser holds the operator token. Unpaired it can read, not change. */
		paired: boolean;
		/** An action is on its way: the actions wait for it. */
		busy?: boolean;
		/** The refusal of the last action, shown where it was asked. */
		actionError?: string | null;
		initialView?: TriageView;
		onView?: (view: TriageView) => void;
		/** A dialog opens: the page drops the last refusal so the dialog does not show it. */
		onClearError?: () => void;
		onRetry?: () => void;
		onPin: (id: number, pinned: boolean) => Promise<boolean>;
		/** `priority` null clears the operator's value. */
		onMove: (id: number, priority: number | null) => Promise<boolean>;
		onMerge: (rootId: number, ids: number[]) => Promise<boolean>;
		onSeparate: (mergeId: number) => Promise<boolean>;
		onGroup: (ids: number[], target: GroupTarget | null) => Promise<boolean>;
		/** A cause's row opened for the first time: load its problems. */
		onOpenCause: (cause: WrightCause) => void;
		onRetryProblems: (causeId: number) => void;
		onShowHeldProblems: (causeId: number) => void;
		onOpenProblem: (problemId: number) => void;
		/** One problem's own detail. */
		problem?: Snippet<[WrightProblem]>;
	}

	const {
		causes,
		status,
		error = null,
		waitedHours,
		groups,
		maxAttempts,
		causeProblems,
		paired,
		busy = false,
		actionError = null,
		initialView = 'ranked',
		onView,
		onClearError,
		onRetry,
		onPin,
		onMove,
		onMerge,
		onSeparate,
		onGroup,
		onOpenCause,
		onRetryProblems,
		onShowHeldProblems,
		onOpenProblem,
		problem
	}: Props = $props();

	/** The view the page remembered; the switch changes it from then on. */
	let view = $state<TriageView>(untrack(() => initialView));

	let selectedIds = $state<number[]>([]);
	/** The selection as the rows now show it: a cause that left the queue drops out. */
	const selected = $derived(causes.filter((c) => selectedIds.includes(c.cause_id)));
	const sections = $derived(sectionsByArea(causes));
	const reason = $derived(paired ? null : 'Pair this browser to change the queue');
	const off = $derived(!paired || busy);

	let mergeOpen = $state(false);
	let groupOpen = $state(false);
	let groupIds = $state<number[]>([]);

	function select(id: number, on: boolean): void {
		selectedIds = on ? [...selectedIds, id] : selectedIds.filter((x) => x !== id);
	}

	function openGroup(ids: number[]): void {
		onClearError?.();
		groupIds = ids;
		groupOpen = true;
	}

	function openMerge(): void {
		onClearError?.();
		mergeOpen = true;
	}

	async function doGroup(target: GroupTarget | null): Promise<boolean> {
		const ok = await onGroup(groupIds, target);
		if (ok) selectedIds = selectedIds.filter((id) => !groupIds.includes(id));
		return ok;
	}

	async function doMerge(rootId: number, ids: number[]): Promise<boolean> {
		const ok = await onMerge(rootId, ids);
		if (ok) selectedIds = [];
		return ok;
	}

	function move(scope: WrightCause[], id: number, direction: MoveDirection): void {
		const priority = moveStep(scope, id, direction);
		if (priority !== null) void onMove(id, priority);
	}

	let dragging = $state<number | null>(null);
	let drop = $state<{ id: number; edge: DropEdge } | null>(null);

	function dragOver(id: number, edge: DropEdge): void {
		if (dragging === null || dropStep(causes, dragging, id, edge) === null) {
			drop = null;
			return;
		}
		drop = { id, edge };
	}

	function dropOn(id: number, edge: DropEdge): void {
		const from = dragging;
		dragging = null;
		drop = null;
		if (from === null) return;
		const priority = dropStep(causes, from, id, edge);
		if (priority !== null) void onMove(from, priority);
	}

	function endDrag(): void {
		dragging = null;
		drop = null;
	}
</script>

{#snippet causeRow(cause: WrightCause, scope: WrightCause[], draggable: boolean)}
	<WrightCauseRow
		{cause}
		{waitedHours}
		disabledReason={reason}
		{busy}
		selected={selectedIds.includes(cause.cause_id)}
		onSelect={(on) => select(cause.cause_id, on)}
		canMoveUp={canMove(scope, cause.cause_id, 'up')}
		canMoveDown={canMove(scope, cause.cause_id, 'down')}
		onPin={(pinned) => void onPin(cause.cause_id, pinned)}
		onMove={(direction) => move(scope, cause.cause_id, direction)}
		onReset={() => void onMove(cause.cause_id, null)}
		onGroup={() => openGroup([cause.cause_id])}
		{draggable}
		dropEdge={drop?.id === cause.cause_id ? drop.edge : null}
		onDragStart={(id) => (dragging = id)}
		onDragOver={dragOver}
		onDrop={dropOn}
		onDragEnd={endDrag}
		onOpen={onOpenCause}
	>
		<WrightCauseDetail
			{cause}
			{maxAttempts}
			problems={causeProblems[cause.cause_id]}
			disabledReason={reason}
			{busy}
			error={actionError}
			onRetry={() => onRetryProblems(cause.cause_id)}
			onShowHeld={() => onShowHeldProblems(cause.cause_id)}
			{onOpenProblem}
			{onClearError}
			{onSeparate}
			{problem}
		/>
	</WrightCauseRow>
{/snippet}

{#if status === 'loading'}
	<WrightNote>Loading.</WrightNote>
{:else if status === 'missing'}
	<WrightNote>Not in the running mike yet.</WrightNote>
{:else if status === 'error'}
	<LoadErrorBanner title="Could not load the triage queue" detail={error ?? ''} {onRetry} />
{:else if causes.length === 0}
	<WrightNote>Nothing to triage. Every open problem has an answer.</WrightNote>
{:else}
	<div class="tri" data-slot="wright-triage">
		<Tabs.Root
			bind:value={view}
			onValueChange={(next) => onView?.(next as TriageView)}
			class="tri__tabs"
		>
			<div class="tri__bar">
				<Tabs.List aria-label="View of the triage queue">
					<Tabs.Trigger value="ranked">Ranked</Tabs.Trigger>
					<Tabs.Trigger value="area">By area</Tabs.Trigger>
				</Tabs.List>
				<!-- Always in the layout: selecting a row shows it without moving the list. -->
				<div
					class="tri__select"
					role="group"
					aria-label="Selected causes"
					data-idle={selected.length === 0 ? '' : undefined}
				>
					<span class="tri__count" aria-live="polite">{selected.length} selected</span>
					<Button
						variant="outline"
						size="sm"
						disabled={off || selected.length < 2}
						title={reason ?? (selected.length < 2 ? 'Select two or more causes' : undefined)}
						onclick={openMerge}
					>
						<GitMerge aria-hidden="true" />
						Merge into
					</Button>
					<Button
						variant="outline"
						size="sm"
						disabled={off || selected.length === 0}
						title={reason ?? undefined}
						onclick={() => openGroup(selected.map((c) => c.cause_id))}
					>
						<FolderPlus aria-hidden="true" />
						Group
					</Button>
					<Button
						variant="ghost"
						size="sm"
						disabled={selected.length === 0}
						onclick={() => (selectedIds = [])}
					>
						<X aria-hidden="true" />
						Clear
					</Button>
				</div>
			</div>

			<div class="tri__notice">
				{#if !paired}
					<WrightNote>
						This browser is not paired, so you can read the queue but not change it. Run
						<code>{PAIR_COMMAND}</code> in a terminal and use the link it opens.
					</WrightNote>
				{:else if actionError !== null && !mergeOpen && !groupOpen}
					<p class="tri__error" role="alert">{actionError}</p>
				{/if}
			</div>

			<Tabs.Content value="ranked">
				{#if view === 'ranked'}
					<ul
						class="tri__rows"
						role="list"
						aria-label="Causes in the order the engineer takes them"
					>
						{#each causes as cause (cause.cause_id)}
							{@render causeRow(cause, causes, paired)}
						{/each}
					</ul>
				{/if}
			</Tabs.Content>

			<Tabs.Content value="area">
				{#if view === 'area'}
					{#each sections as section (section.key)}
						<section class="tri__section" aria-labelledby="tri-section-{section.key}">
							<h3 class="machine tri__section-h" id="tri-section-{section.key}">
								{#if section.kind === 'group'}<Folder size={12} aria-hidden="true" />{/if}
								<span class="tri__section-name">{section.label}</span>
								<span class="tri__section-meta">
									{section.causes.length}
									{section.causes.length === 1 ? 'cause' : 'causes'} · {problemsText(
										section.causes.reduce((sum, c) => sum + c.problems, 0)
									)}
								</span>
							</h3>
							{#if section.detail.length > 0}
								<p class="tri__section-detail">{section.detail.join(', ')}</p>
							{/if}
							<ul class="tri__rows" role="list" aria-label="Causes in {section.label}">
								{#each section.causes as cause (cause.cause_id)}
									{@render causeRow(cause, section.causes, false)}
								{/each}
							</ul>
						</section>
					{/each}
				{/if}
			</Tabs.Content>
		</Tabs.Root>
	</div>

	<WrightMergeDialog
		bind:open={mergeOpen}
		causes={selected}
		{busy}
		error={actionError}
		onConfirm={doMerge}
	/>
	<WrightGroupDialog
		bind:open={groupOpen}
		count={groupIds.length}
		{groups}
		{busy}
		error={actionError}
		onConfirm={doGroup}
	/>
{/if}

<style>
	.tri :global(.tri__tabs) {
		gap: var(--space-2);
	}

	.tri__bar {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2) var(--space-4);
	}

	.tri__select {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	/* No selection: the bar stays, unseen, so selecting a row moves nothing. */
	.tri__select[data-idle] {
		visibility: hidden;
	}

	.tri__count {
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-2);
	}

	/* One line is reserved for the pairing note and for a refusal. */
	.tri__notice {
		min-block-size: 1.4em;
	}

	.tri__error {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		color: var(--status-danger-fg);
		overflow-wrap: anywhere;
	}

	.tri__rows {
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.tri__section {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding-block-start: var(--space-3);
	}

	.tri__section-h {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin: 0;
		font-size: var(--text-xs);
		color: var(--text-2);
	}

	/* A table schema, a module path or a job name keeps its own case and spelling. */
	.tri__section-name {
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		font-weight: 600;
		letter-spacing: normal;
		text-transform: none;
		color: var(--text-strong);
		overflow-wrap: anywhere;
	}

	.tri__section-meta {
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		letter-spacing: normal;
		text-transform: none;
		color: var(--text-3);
	}

	.tri__section-detail {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-3);
		overflow-wrap: anywhere;
	}
</style>
