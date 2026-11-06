<script lang="ts">
	import type { Snippet } from 'svelte';
	import { slide } from 'svelte/transition';
	import { resolve } from '$app/paths';
	import { ArrowUpRight, Folder, FolderOpen, GripVertical, Settings } from '@lucide/svelte';
	import type { FleetProjectWithCounts } from '$lib/fleet/types';
	import {
		buildProjectTree,
		flattenVisibleProjectTree,
		folderLabel,
		isUnknownProjectPath,
		projectMatchesQuery,
		pruneVisibleProjectTree,
		type ProjectFolderRow,
		type ProjectLeafRow
	} from '$lib/fleet/projectTree';
	import { pinnedProjectOrder, computeDisplayOrder } from '$lib/stores/pinnedProjectOrder.svelte';
	import { formatRelative, formatCount, formatClockAbsolute } from '$lib/time';
	import PinToggle from '$lib/components/shared/PinToggle.svelte';

	interface Props {
		projects: readonly FleetProjectWithCounts[];
		query?: string;
		expandedPath: string | null;
		openDuration?: number;
		unknownExpandable?: boolean;
		reconcileOrder?: boolean;
		showRowCounts?: boolean;
		alwaysShowPath?: boolean;
		rowMode?: 'link' | 'toggle';
		indentStep?: number;
		primaryNoun?: string;
		secondaryNoun?: string;
		activityLabel?: string;
		unknownNote?: string | null;
		now?: number;
		onToggleProject: (path: string) => void;
		onOpenSettings: (path: string) => void;
		onTogglePin: (path: string, next: boolean) => void;
		body?: Snippet<[FleetProjectWithCounts]>;
	}

	const {
		projects,
		query = '',
		expandedPath,
		openDuration = 0,
		unknownExpandable = false,
		reconcileOrder = true,
		showRowCounts = false,
		alwaysShowPath = false,
		rowMode = 'link',
		indentStep = 20,
		primaryNoun = 'session',
		secondaryNoun = 'request',
		activityLabel = 'last seen',
		unknownNote,
		now,
		onToggleProject,
		onOpenSettings,
		onTogglePin,
		body
	}: Props = $props();

	const DEFAULT_UNKNOWN_NOTE =
		'Captures with no resolvable real project path — a scratch/temp working directory, or traffic with no project context at all. Still fully inspectable; the real path is shown below each entry.';
	const UNKNOWN_SENTINEL = '(no project)';

	const toggleMode = $derived(rowMode === 'toggle');
	let collapsedFolders = $state<Set<string>>(new Set());

	function projectHref(path: string): `/projects/${string}` {
		return `/projects/${encodeURIComponent(path)}`;
	}

	function countLabel(count: number, noun: string): string {
		return `${formatCount(count)} ${noun}${count === 1 ? '' : 's'}`;
	}

	function rollupLabel(sessions: number, requests: number): string {
		return `${countLabel(sessions, primaryNoun)} · ${countLabel(requests, secondaryNoun)}`;
	}

	function toggleFolder(path: string): void {
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- cloned, mutated, then reassigned wholesale; reactivity comes from the reassignment below, not in-place mutation
		const next = new Set(collapsedFolders);
		if (next.has(path)) next.delete(path);
		else next.add(path);
		collapsedFolders = next;
	}

	const normalizedQuery = $derived(query.trim().toLowerCase());
	const matches = $derived((p: FleetProjectWithCounts) => projectMatchesQuery(p, normalizedQuery));

	const searchedProjects = $derived(projects.filter(matches));
	const realProjectsUnfiltered = $derived(
		projects.filter((p) => !isUnknownProjectPath(p.project_path))
	);
	const realProjects = $derived(
		searchedProjects.filter((p) => !isUnknownProjectPath(p.project_path))
	);
	const unknownProjects = $derived(
		searchedProjects
			.filter((p) => isUnknownProjectPath(p.project_path))
			.toSorted((a, b) => {
				if (unknownExpandable) {
					const aLast = a.project_path === UNKNOWN_SENTINEL ? 1 : 0;
					const bLast = b.project_path === UNKNOWN_SENTINEL ? 1 : 0;
					if (aLast !== bLast) return aLast - bLast;
				}
				return new Date(b.last_seen).getTime() - new Date(a.last_seen).getTime();
			})
	);

	const projectTree = $derived(buildProjectTree(realProjects));
	const visibleProjectTree = $derived(pruneVisibleProjectTree(projectTree));
	const treeRows = $derived(flattenVisibleProjectTree(visibleProjectTree, collapsedFolders));

	const pinnedProjectsUnfiltered = $derived(realProjectsUnfiltered.filter((p) => p.pinned));
	const pinnedPaths = $derived(pinnedProjectsUnfiltered.map((p) => p.project_path));
	const pinnedByPath = $derived(new Map(pinnedProjectsUnfiltered.map((p) => [p.project_path, p])));
	const pinnedOrderedPaths = $derived(computeDisplayOrder(pinnedProjectOrder.order, pinnedPaths));
	const pinnedRows = $derived(
		pinnedOrderedPaths
			.map((path) => pinnedByPath.get(path))
			.filter((p): p is FleetProjectWithCounts => p !== undefined)
			.filter(matches)
	);

	interface PinnedClusterMember {
		project: FleetProjectWithCounts;
		index: number;
	}
	interface PinnedGroupItem {
		kind: 'group';
		parentPath: string;
		members: PinnedClusterMember[];
	}
	interface PinnedSingleItem {
		kind: 'single';
		member: PinnedClusterMember;
	}
	type PinnedDisplayItem = PinnedGroupItem | PinnedSingleItem;

	function parentPathOf(path: string): string | null {
		const segments = path.split('/');
		if (segments.length <= 1) return null;
		segments.pop();
		return segments.join('/');
	}

	function tailSegmentOf(path: string): string {
		const idx = path.lastIndexOf('/');
		return idx === -1 ? path : path.slice(idx + 1);
	}

	function partitionPinnedDisplay(rows: readonly FleetProjectWithCounts[]): PinnedDisplayItem[] {
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- local lookup table inside this function; never stored in state or read reactively
		const parentCounts = new Map<string, number>();
		for (const p of rows) {
			const parent = parentPathOf(p.project_path);
			if (parent !== null) parentCounts.set(parent, (parentCounts.get(parent) ?? 0) + 1);
		}

		const items: PinnedDisplayItem[] = [];
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- local lookup table inside this function; never stored in state or read reactively
		const groups = new Map<string, PinnedGroupItem>();

		rows.forEach((project, index) => {
			const parent = parentPathOf(project.project_path);
			if (parent === null || (parentCounts.get(parent) ?? 0) < 2) {
				items.push({ kind: 'single', member: { project, index } });
				return;
			}
			let group = groups.get(parent);
			if (!group) {
				group = { kind: 'group', parentPath: parent, members: [] };
				groups.set(parent, group);
				items.push(group);
			}
			group.members.push({ project, index });
		});

		return items;
	}

	const pinnedDisplayItems = $derived(partitionPinnedDisplay(pinnedRows));

	$effect(() => {
		if (!reconcileOrder) return;
		pinnedProjectOrder.reconcile(pinnedPaths);
	});

	const expandedProject = $derived(projects.find((p) => p.project_path === expandedPath) ?? null);

	function handleProjectClick(event: MouseEvent, path: string): void {
		if (
			event.defaultPrevented ||
			event.button !== 0 ||
			event.metaKey ||
			event.ctrlKey ||
			event.shiftKey ||
			event.altKey
		) {
			return;
		}
		event.preventDefault();
		onToggleProject(path);
	}

	function openSettings(event: MouseEvent, path: string): void {
		event.stopPropagation();
		event.preventDefault();
		onOpenSettings(path);
	}

	let dragIndex = $state<number | null>(null);
	let dropIndex = $state<number | null>(null);

	function handleDragStart(index: number, event: DragEvent): void {
		dragIndex = index;
		if (event.dataTransfer) {
			event.dataTransfer.effectAllowed = 'move';
			event.dataTransfer.setData('text/plain', String(index));
		}
	}

	function handleDragOver(index: number, event: DragEvent): void {
		if (dragIndex === null) return;
		event.preventDefault();
		if (event.dataTransfer) event.dataTransfer.dropEffect = 'move';
		dropIndex = index;
	}

	function handleDrop(index: number, event: DragEvent): void {
		event.preventDefault();
		if (dragIndex !== null && dragIndex !== index) {
			reorderPinned(dragIndex, index);
		}
		dragIndex = null;
		dropIndex = null;
	}

	function handleDragEnd(): void {
		dragIndex = null;
		dropIndex = null;
	}

	function commitOrder(visible: readonly string[]): void {
		const shown = new Set(visible);
		const rest = pinnedProjectOrder.order.filter((p) => !shown.has(p));
		pinnedProjectOrder.reorder([...visible, ...rest]);
	}

	function reorderPinned(fromIndex: number, toIndex: number): void {
		const current = [...pinnedOrderedPaths];
		const moved = current[fromIndex];
		if (moved === undefined) return;
		current.splice(fromIndex, 1);
		current.splice(toIndex, 0, moved);
		commitOrder(current);
	}

	function movePinned(path: string, delta: -1 | 1): void {
		const current = [...pinnedOrderedPaths];
		const idx = current.indexOf(path);
		if (idx === -1) return;
		const nextIdx = idx + delta;
		if (nextIdx < 0 || nextIdx >= current.length) return;
		const moved = current[idx]!;
		current.splice(idx, 1);
		current.splice(nextIdx, 0, moved);
		commitOrder(current);
	}

	function onPinnedHandleKeydown(event: KeyboardEvent, path: string): void {
		if (event.key === 'ArrowUp') {
			event.preventDefault();
			movePinned(path, -1);
		} else if (event.key === 'ArrowDown') {
			event.preventDefault();
			movePinned(path, 1);
		}
	}
</script>

{#snippet breakable(text: string)}
	{#each text.split('/') as segment, i (i)}{#if i > 0}/<wbr />{/if}{segment}{/each}
{/snippet}

{#snippet projectVitals(p: FleetProjectWithCounts, showPinnedBadge: boolean)}
	{#if showRowCounts}
		<span class="pt-row-counts font-mono" data-slot="project-row-counts"
			>{rollupLabel(p.session_count, p.request_count)}</span
		>
	{/if}
	{#if showPinnedBadge && p.pinned}
		<span class="badge-nx badge-active" data-slot="project-row-pinned">Pinned</span>
	{/if}
	<span
		class="pt-last-seen"
		data-slot="project-row-last-seen"
		title={formatClockAbsolute(p.last_seen)}
	>
		{activityLabel} <span class="font-mono">{formatRelative(p.last_seen, now)}</span>
	</span>
{/snippet}

{#snippet expandedDetail(p: FleetProjectWithCounts, inPinnedList: boolean)}
	<div
		class="pt-expanded"
		role={inPinnedList ? 'listitem' : undefined}
		data-slot="project-row-expanded"
		data-project-path={p.project_path}
		transition:slide={{ duration: openDuration }}
	>
		{@render body?.(p)}
	</div>
{/snippet}

{#snippet rowContent(
	p: FleetProjectWithCounts,
	name: string,
	showPath: boolean,
	showBadge: boolean,
	slotted: boolean
)}
	<span class="pt-project-name font-mono" data-slot={slotted ? 'project-row-path' : undefined}
		>{@render breakable(name)}</span
	>
	{#if showPath}
		<span class="pt-project-path font-mono" data-slot={slotted ? 'project-row-fullpath' : undefined}
			>{@render breakable(p.project_path)}</span
		>
	{/if}
	{@render projectVitals(p, showBadge)}
{/snippet}

{#snippet openProjectLink(path: string, name: string)}
	{#if toggleMode}
		<a
			class="inline-flex size-6 items-center justify-center rounded-[2px] text-[var(--text-3)] hover:bg-[var(--surface-card)] hover:text-[var(--text-strong)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--ring)]"
			href={resolve(projectHref(path))}
			aria-label={`Open project ${name}`}
			title="Open the project"
			data-slot="project-open-link"><ArrowUpRight size={12} aria-hidden="true" /></a
		>
	{/if}
{/snippet}

{#snippet gearButton(path: string)}
	<button
		type="button"
		class="inline-flex size-6 items-center justify-center rounded-[2px] text-[var(--text-3)] transition-colors hover:bg-[var(--surface-card)] hover:text-[var(--text-strong)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--ring)]"
		aria-label="Edit project"
		title="Edit project"
		data-slot="project-settings-open"
		onclick={(e) => openSettings(e, path)}
	>
		<Settings size={12} aria-hidden="true" />
	</button>
{/snippet}

{#snippet pinnedRow(p: FleetProjectWithCounts, i: number, displayName: string)}
	<div
		class="pt-pinned-row"
		class:pt-pinned-row--expanded={expandedPath === p.project_path}
		class:pt-pinned-row--dragging={dragIndex === i}
		class:pt-pinned-row--drop-target={dropIndex === i && dragIndex !== null && dragIndex !== i}
		role="listitem"
		ondragover={(e) => handleDragOver(i, e)}
		ondrop={(e) => handleDrop(i, e)}
	>
		<button
			type="button"
			class="pt-drag-handle"
			aria-label={`Reorder pinned project ${p.project_path} — drag, or press arrow up / arrow down`}
			title="Drag to reorder (or use arrow keys)"
			draggable="true"
			ondragstart={(e) => handleDragStart(i, e)}
			ondragend={handleDragEnd}
			onkeydown={(e) => onPinnedHandleKeydown(e, p.project_path)}
		>
			<GripVertical size={12} aria-hidden="true" />
		</button>
		{#if toggleMode}
			<button
				type="button"
				class="pt-project-link pt-project-link--button"
				class:pt-project-link--expanded={expandedPath === p.project_path}
				data-slot="project-row"
				data-project-path={p.project_path}
				title={p.project_path}
				aria-label={p.custom_name ?? p.project_path}
				aria-expanded={expandedPath === p.project_path}
				onclick={() => onToggleProject(p.project_path)}
			>
				{@render rowContent(
					p,
					p.custom_name ?? displayName,
					Boolean(p.custom_name) || alwaysShowPath,
					false,
					true
				)}
			</button>
		{:else}
			<a
				href={resolve(projectHref(p.project_path))}
				class="pt-project-link"
				class:pt-project-link--expanded={expandedPath === p.project_path}
				data-slot="project-row"
				data-project-path={p.project_path}
				title={p.project_path}
				aria-label={p.custom_name ?? p.project_path}
				aria-expanded={expandedPath === p.project_path}
				onclick={(e) => handleProjectClick(e, p.project_path)}
			>
				{@render rowContent(
					p,
					p.custom_name ?? displayName,
					Boolean(p.custom_name) || alwaysShowPath,
					false,
					true
				)}
			</a>
		{/if}
		{@render openProjectLink(p.project_path, p.custom_name ?? displayName)}
		{@render gearButton(p.project_path)}
		<PinToggle
			pinned={true}
			ariaLabel="Unpin project"
			onToggle={() => onTogglePin(p.project_path, false)}
		/>
	</div>
	{#if expandedPath === p.project_path && expandedProject !== null}
		{@render expandedDetail(expandedProject, true)}
	{/if}
{/snippet}

<div class="pt-panels" data-slot="projects-panels">
	<div class="pt-panel funk-sec edge" data-accent="teal" data-slot="projects-directory">
		<div class="funk-sec-body pt-list">
			{#if pinnedRows.length > 0}
				<div
					class="pt-pinned-list"
					role="list"
					aria-label="Pinned projects, in your chosen order"
					data-slot="projects-pinned"
				>
					{#each pinnedDisplayItems as item (item.kind === 'group' ? `group:${item.parentPath}` : `single:${item.member.project.project_path}`)}
						{#if item.kind === 'group'}
							<p class="pt-pinned-group-header font-mono" aria-hidden="true">
								{@render breakable(item.parentPath)}
							</p>
							{#each item.members as member (member.project.project_path)}
								{@render pinnedRow(
									member.project,
									member.index,
									tailSegmentOf(member.project.project_path)
								)}
							{/each}
						{:else}
							{@render pinnedRow(
								item.member.project,
								item.member.index,
								item.member.project.project_path
							)}
						{/if}
					{/each}
				</div>
			{/if}

			<div class="pt-tree" data-slot="projects-tree">
				{#if treeRows.length === 0}
					<p class="text-label text-[var(--text-3)]" data-slot="projects-tree-empty">
						No classified projects — see Pinned above / Unclassified below.
					</p>
				{/if}
				{#each treeRows as row, index (row.kind === 'separator' ? `sep-${index}` : `${row.kind}:${row.node.path}`)}
					{#if row.kind === 'folder'}
						{@const r = row as ProjectFolderRow}
						{@const isCollapsed = collapsedFolders.has(r.node.path)}
						<div
							class="pt-row pt-row--folder"
							class:pt-row--connected-before={r.connectsBefore}
							style={`--tree-indent: ${r.depth * indentStep}px`}
						>
							<button
								type="button"
								class="pt-folder-button"
								aria-expanded={!isCollapsed}
								aria-label={`${isCollapsed ? 'Expand' : 'Collapse'} folder ${r.node.path}`}
								data-folder-path={r.node.path}
								title={r.node.path}
								onclick={() => toggleFolder(r.node.path)}
							>
								{#if isCollapsed}
									<Folder size={14} aria-hidden="true" />
								{:else}
									<FolderOpen size={14} aria-hidden="true" />
								{/if}
								<span class="pt-folder-label font-mono"
									>{@render breakable(folderLabel(r.node))}</span
								>
								<span class="pt-folder-meta">
									<span class="pt-folder-rollup font-mono">
										{rollupLabel(r.node.sessionCount, r.node.requestCount)}
									</span>
									<span class="pt-folder-last-seen" title={formatClockAbsolute(r.node.lastSeen)}>
										{activityLabel}
										<span class="font-mono">{formatRelative(r.node.lastSeen, now)}</span>
									</span>
								</span>
							</button>
						</div>
					{:else if row.kind === 'leaf'}
						{@const r = row as ProjectLeafRow}
						{@const isExpanded = expandedPath === r.node.path}
						<div class="pt-row" style={`--tree-indent: ${r.depth * indentStep}px`}>
							{#if toggleMode}
								<div
									class="pt-project-link pt-project-link--line"
									class:pt-project-link--expanded={isExpanded}
								>
									<button
										type="button"
										class="pt-project-main"
										data-slot="project-row"
										data-project-path={r.node.path}
										title={r.node.path}
										aria-expanded={isExpanded}
										onclick={() => onToggleProject(r.node.path)}
									>
										{@render rowContent(
											r.node.project,
											r.node.project.custom_name ?? r.node.name,
											Boolean(r.node.project.custom_name) || alwaysShowPath,
											true,
											true
										)}
									</button>
									{@render gearButton(r.node.path)}
									{@render openProjectLink(r.node.path, r.node.project.custom_name ?? r.node.name)}
									<PinToggle
										pinned={r.node.project.pinned}
										ariaLabel={r.node.project.pinned ? 'Unpin project' : 'Pin project'}
										onToggle={() => onTogglePin(r.node.path, !r.node.project.pinned)}
									/>
								</div>
							{:else}
								<a
									href={resolve(projectHref(r.node.path))}
									class="pt-project-link"
									class:pt-project-link--expanded={isExpanded}
									data-slot="project-row"
									data-project-path={r.node.path}
									title={r.node.path}
									aria-expanded={isExpanded}
									onclick={(e) => handleProjectClick(e, r.node.path)}
								>
									{@render rowContent(
										r.node.project,
										r.node.project.custom_name ?? r.node.name,
										Boolean(r.node.project.custom_name) || alwaysShowPath,
										true,
										true
									)}
									{@render gearButton(r.node.path)}
									<PinToggle
										pinned={r.node.project.pinned}
										ariaLabel={r.node.project.pinned ? 'Unpin project' : 'Pin project'}
										onToggle={() => onTogglePin(r.node.path, !r.node.project.pinned)}
									/>
								</a>
							{/if}
							{#if isExpanded && expandedProject !== null}
								{@render expandedDetail(expandedProject, false)}
							{/if}
						</div>
					{:else}
						<div
							class="pt-separator"
							style={`--tree-indent: ${row.depth * indentStep}px`}
							aria-hidden="true"
						></div>
					{/if}
				{/each}
			</div>
		</div>
	</div>

	{#if unknownProjects.length > 0}
		<div class="pt-panel funk-sec edge" data-accent="muted" data-slot="projects-unknown">
			<div class="funk-sec-body">
				{#if unknownNote !== null}
					<p class="pt-unknown-note">{unknownNote ?? DEFAULT_UNKNOWN_NOTE}</p>
				{/if}
				<div class="pt-unknown-list">
					{#each unknownProjects as p (p.project_path)}
						{@const isExpanded = unknownExpandable && expandedPath === p.project_path}
						{#if toggleMode && unknownExpandable}
							<button
								type="button"
								class="pt-project-link pt-project-link--unknown pt-project-link--button"
								class:pt-project-link--expanded={isExpanded}
								data-slot="project-row"
								data-project-path={p.project_path}
								aria-expanded={isExpanded}
								onclick={() => onToggleProject(p.project_path)}
							>
								{@render rowContent(p, p.custom_name ?? 'Unknown', true, true, false)}
							</button>
						{:else}
							<a
								href={resolve(projectHref(p.project_path))}
								class="pt-project-link pt-project-link--unknown"
								class:pt-project-link--expanded={isExpanded}
								data-slot="project-row"
								data-project-path={p.project_path}
								aria-expanded={unknownExpandable ? isExpanded : undefined}
								onclick={unknownExpandable
									? (e: MouseEvent) => handleProjectClick(e, p.project_path)
									: undefined}
							>
								{@render rowContent(p, p.custom_name ?? 'Unknown', true, true, false)}
							</a>
						{/if}
						{#if isExpanded && expandedProject !== null}
							{@render expandedDetail(expandedProject, false)}
						{/if}
					{/each}
				</div>
			</div>
		</div>
	{/if}
</div>

<style>
	.pt-panels {
		display: flex;
		flex-direction: column;
		flex: 0 0 auto;
		background: var(--surface-card);
	}
	.pt-panels > .pt-panel + .pt-panel {
		margin-top: -1px;
	}
	.pt-panels > .pt-panel:not(:first-child) {
		--sec-tl: 0px;
	}
	.pt-panels > .pt-panel:not(:last-child) {
		--sec-br: 0px;
	}

	.pt-tree {
		display: flex;
		min-width: 0;
		flex-direction: column;
	}

	.pt-list {
		display: flex;
		min-width: 0;
		flex-direction: column;
	}
	.pt-list > .pt-pinned-list + .pt-tree {
		border-top: 1px solid var(--border-subtle);
		margin-top: 4px;
		padding-top: 4px;
	}

	.pt-row {
		min-width: 0;
		border-top: 1px solid var(--border-subtle);
	}

	.pt-tree > .pt-row:first-child,
	.pt-separator + .pt-row,
	.pt-row--connected-before {
		border-top: none;
	}

	.pt-separator {
		height: 1px;
		margin: 6px 0 6px calc(var(--tree-indent, 0px) + 14px);
		border-top: 1px solid var(--border-subtle);
	}

	.pt-folder-button {
		display: flex;
		width: calc(100% - var(--tree-indent, 0px));
		margin-left: var(--tree-indent, 0px);
		flex-wrap: wrap;
		align-items: center;
		column-gap: 8px;
		row-gap: 2px;
		padding: 7px 12px;
		border: none;
		background: transparent;
		color: var(--text-2);
		font: inherit;
		text-align: left;
		cursor: pointer;
	}

	.pt-folder-button:hover {
		background: var(--surface-2);
		color: var(--text-1);
	}

	.pt-folder-button:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}

	.pt-folder-button :global(svg) {
		flex-shrink: 0;
		color: var(--text-3);
	}

	.pt-folder-label {
		min-width: 0;
		flex: 1 1 auto;
		overflow-wrap: break-word;
		font-weight: 600;
		color: var(--text-1);
	}

	.pt-folder-meta {
		display: flex;
		flex-shrink: 0;
		flex-wrap: wrap;
		align-items: center;
		column-gap: 8px;
	}

	.pt-folder-rollup,
	.pt-folder-last-seen,
	.pt-row-counts {
		flex-shrink: 0;
		font-size: 11px;
		color: var(--text-3);
	}

	@media (max-width: 640px) {
		.pt-folder-meta {
			flex-basis: 100%;
			padding-left: 22px;
		}
	}

	.pt-project-link {
		display: flex;
		width: calc(100% - var(--tree-indent, 0px));
		margin-left: var(--tree-indent, 0px);
		flex-wrap: wrap;
		align-items: center;
		column-gap: 12px;
		row-gap: 2px;
		border: none;
		background: transparent;
		padding: 7px 12px;
		text-decoration: none;
		color: inherit;
	}

	.pt-project-link:hover {
		background: var(--surface-2);
	}

	.pt-project-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}

	.pt-project-name {
		min-width: 0;
		flex: 0 1 auto;
		overflow-wrap: break-word;
		font-size: 13px;
		line-height: 1.3;
		font-weight: 600;
		color: var(--text-strong);
	}

	.pt-last-seen {
		flex-shrink: 0;
		font-size: 11px;
		line-height: 1.3;
		color: var(--text-3);
	}

	.pt-pinned-list {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.pt-pinned-group-header {
		margin: 6px 0 0 4px;
		overflow-wrap: break-word;
		font-size: 11px;
		color: var(--text-3);
	}

	.pt-pinned-group-header:first-child {
		margin-top: 0;
	}

	.pt-pinned-row {
		display: flex;
		align-items: center;
		gap: 8px;
		border-radius: 6px;
		transition:
			opacity 120ms ease,
			box-shadow 120ms ease;
	}

	.pt-pinned-row--dragging {
		opacity: 0.4;
	}

	.pt-pinned-row--drop-target {
		box-shadow: 0 0 0 1px var(--interactive-active-border);
	}

	.pt-pinned-row .pt-project-link {
		flex: 1;
		min-width: 0;
		width: auto;
		margin-left: 0;
	}

	.pt-drag-handle {
		display: inline-flex;
		flex-shrink: 0;
		align-items: center;
		justify-content: center;
		width: 22px;
		height: 22px;
		padding: 2px;
		color: var(--text-3);
		background: transparent;
		border: none;
		border-radius: 4px;
		cursor: grab;
	}
	.pt-drag-handle:hover {
		color: var(--text-strong);
		background: var(--surface-2);
	}
	.pt-drag-handle:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}
	.pt-drag-handle:active {
		cursor: grabbing;
	}

	.pt-unknown-note {
		margin-bottom: 10px;
		font-size: 11px;
		color: var(--text-3);
	}

	.pt-project-link--button {
		box-sizing: border-box;
		font: inherit;
		text-align: left;
		cursor: pointer;
	}

	.pt-project-link--line {
		align-items: center;
		column-gap: var(--space-2);
		padding: 0 12px 0 0;
	}

	.pt-project-main {
		all: unset;
		box-sizing: border-box;
		display: flex;
		flex: 1 1 12rem;
		min-width: 0;
		flex-wrap: wrap;
		align-items: center;
		column-gap: 12px;
		row-gap: 2px;
		padding: 7px 12px;
		cursor: pointer;
	}

	.pt-project-main:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}

	.pt-unknown-list {
		display: flex;
		flex-direction: column;
	}

	.pt-project-link--unknown {
		background: transparent;
	}

	.pt-unknown-list > * + .pt-project-link--unknown {
		border-top: var(--hairline) solid var(--border-subtle);
	}

	.pt-project-link--unknown .pt-project-name {
		flex: 0 0 auto;
		color: var(--text-2);
	}

	.pt-project-path {
		min-width: 0;
		flex: 1 1 auto;
		overflow-wrap: break-word;
		font-size: 12px;
		line-height: 1.3;
		color: var(--text-3);
	}

	.pt-expanded {
		display: flex;
		flex-direction: column;
		gap: var(--space-3, 12px);
		margin-left: calc(var(--tree-indent, 0px) + 12px);
		padding: var(--space-2, 8px) 0 var(--space-4, 16px) var(--space-3, 12px);
		border-left: 2px solid var(--funk-cyan-border);
	}

	.pt-project-link {
		transition:
			background-color var(--dur-1, 120ms) var(--ease-io, ease),
			padding-block 180ms var(--ease-io, ease);
	}
	.pt-project-link--expanded {
		background: var(--surface-2);
		padding-block: 13px;
	}

	.pt-pinned-row--expanded {
		background: var(--surface-2);
	}
	.pt-project-link--line.pt-project-link--expanded {
		padding-block: 0;
	}
	.pt-project-link--line.pt-project-link--expanded .pt-project-main {
		padding-block: 13px;
	}

	.pt-pinned-row--expanded .pt-project-link--expanded {
		background: none;
	}
</style>
