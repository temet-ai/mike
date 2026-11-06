<script lang="ts">
	import { List, NotebookText, Waypoints, FolderGit2, X } from '@lucide/svelte';
	import { resolve } from '$app/paths';
	import { roleTokens } from './roleTokens';
	import type { FleetProjectSummary } from './types';

	export type FleetView = 'board' | 'relationships' | 'notebook';

	export interface ClusterChip {
		hubId: string;
		label: string;
		role: string | null;
		agentType: string | null;
		count: number;
		errorCount: number;
	}

	interface Props {
		projects: FleetProjectSummary[];
		selectedProject: string;
		onProjectChange: (project: string) => void;
		windowHours: number;
		onWindowHoursChange: (hours: number) => void;
		searchQuery: string;
		onSearchQueryChange: (query: string) => void;
		onSearchSubmit: () => void;
		searchMatchCount: number;
		view: FleetView;
		onViewChange: (view: FleetView) => void;
		relationshipsAvailable: boolean;
		connectedCount: number;
		clusters: ClusterChip[];
		selectedClusterHubId: string | null;
		onClusterChange: (hubId: string) => void;
	}

	const {
		projects,
		selectedProject,
		onProjectChange,
		windowHours,
		onWindowHoursChange,
		searchQuery,
		onSearchQueryChange,
		onSearchSubmit,
		searchMatchCount,
		view,
		onViewChange,
		relationshipsAvailable,
		connectedCount,
		clusters,
		selectedClusterHubId,
		onClusterChange
	}: Props = $props();

	const trimmedQuery = $derived(searchQuery.trim());
	const showNoMatchHint = $derived(trimmedQuery.length > 0 && searchMatchCount === 0);
	const showMatchCountHint = $derived(trimmedQuery.length > 0 && searchMatchCount > 0);
	const showClusterRow = $derived(view === 'relationships' && clusters.length >= 2);

	function clusterBorderBg(active: boolean, borderVar: string): string {
		return active
			? `linear-gradient(135deg, ${borderVar} 0%, var(--border-default) 70%)`
			: 'var(--border-default)';
	}
</script>

<div class="fleet-topbar" data-slot="fleet-topbar">
	<div class="fleet-topbar__row">
		<div class="fleet-topbar__brand">FLEET</div>

		<label class="fleet-topbar__field">
			<span>project</span>
			<select
				value={selectedProject}
				disabled={projects.length === 0}
				onchange={(e) => onProjectChange(e.currentTarget.value)}
				data-slot="fleet-project-select"
			>
				{#if !projects.some((project) => project.project_path === selectedProject)}
					<option value={selectedProject}>{selectedProject || 'No project selected'}</option>
				{/if}
				{#each projects as p (p.project_path)}
					<option value={p.project_path}>{p.project_path}</option>
				{/each}
			</select>
		</label>
		{#if selectedProject}
			<a
				class="fleet-topbar__project-link"
				href={resolve(`/projects/${encodeURIComponent(selectedProject)}` as const)}
				aria-label="View project page for {selectedProject}"
				title="View project page"
				data-slot="fleet-project-open"
			>
				<FolderGit2 size={13} aria-hidden="true" />
			</a>
		{/if}
		<label class="fleet-topbar__field">
			<span>window</span>
			<select
				value={String(windowHours)}
				onchange={(e) => onWindowHoursChange(Number(e.currentTarget.value))}
				data-slot="fleet-window-select"
			>
				<option value="24">24h</option>
				<option value="72">72h</option>
				<option value="168">7d</option>
				<option value="720">30d</option>
			</select>
		</label>

		<label class="fleet-topbar__field">
			<span class="fleet-topbar__search-label">find</span>
			<span class="fleet-topbar__search-wrap">
				<input
					type="text"
					class="fleet-topbar__search"
					placeholder="find session..."
					value={searchQuery}
					oninput={(e) => onSearchQueryChange(e.currentTarget.value)}
					onkeydown={(e) => {
						if (e.key === 'Enter') {
							e.preventDefault();
							onSearchSubmit();
						} else if (e.key === 'Escape') {
							e.preventDefault();
							onSearchQueryChange('');
						}
					}}
					aria-label="Find session"
					data-slot="fleet-search-input"
				/>
				{#if trimmedQuery.length > 0}
					<button
						type="button"
						class="fleet-topbar__search-clear"
						aria-label="Clear search"
						onclick={() => onSearchQueryChange('')}
						data-slot="fleet-search-clear"
					>
						<X size={12} aria-hidden="true" />
					</button>
				{/if}
			</span>
		</label>
		{#if showNoMatchHint}
			<span class="fleet-topbar__no-match" data-slot="fleet-search-no-match"
				>no match in this view</span
			>
		{:else if showMatchCountHint}
			<span class="fleet-topbar__match-count" data-slot="fleet-search-match-count">
				{searchMatchCount} match{searchMatchCount === 1 ? '' : 'es'}
			</span>
		{/if}

		<div
			class="fleet-topbar__view"
			role="group"
			aria-label="Fleet view"
			data-slot="fleet-view-switch"
		>
			<button
				type="button"
				class="fleet-topbar__view-btn"
				class:fleet-topbar__view-btn--active={view === 'relationships'}
				aria-pressed={view === 'relationships'}
				disabled={!relationshipsAvailable}
				onclick={() => onViewChange('relationships')}
				data-slot="fleet-view-relationships"
			>
				<Waypoints size={14} aria-hidden="true" />
				Relationships ({connectedCount})
			</button>
			<button
				type="button"
				class="fleet-topbar__view-btn"
				class:fleet-topbar__view-btn--active={view === 'board'}
				aria-pressed={view === 'board'}
				onclick={() => onViewChange('board')}
				data-slot="fleet-view-board"
			>
				<List size={14} aria-hidden="true" />
				Board
			</button>
			<button
				type="button"
				class="fleet-topbar__view-btn"
				class:fleet-topbar__view-btn--active={view === 'notebook'}
				aria-pressed={view === 'notebook'}
				onclick={() => onViewChange('notebook')}
				data-slot="fleet-view-notebook"
			>
				<NotebookText size={14} aria-hidden="true" />
				Notebook
			</button>
		</div>
	</div>

	{#if showClusterRow}
		<div
			class="fleet-topbar__clusters"
			role="group"
			aria-label="Fleet clusters"
			data-slot="fleet-cluster-switch"
		>
			<span class="fleet-topbar__clusters-label">clusters</span>
			{#each clusters as cluster (cluster.hubId)}
				{@const tokens = roleTokens(cluster.role)}
				{@const Icon = tokens.icon}
				<span class="fleet-cluster-chip-backing">
					<button
						type="button"
						class="fleet-cluster-chip"
						class:fleet-cluster-chip--active={cluster.hubId === selectedClusterHubId}
						onclick={() => onClusterChange(cluster.hubId)}
						data-slot="fleet-cluster-chip"
						data-cluster-hub={cluster.hubId}
						data-augmented-ui="tl-clip br-clip border"
						style:--aug-border-bg={clusterBorderBg(
							cluster.hubId === selectedClusterHubId,
							tokens.border
						)}
					>
						<span class="fleet-cluster-chip__icon" style:color={tokens.fg}
							><Icon size={12} aria-hidden="true" /></span
						>
						<span class="fleet-cluster-chip__label">{tokens.label}</span>
						<span class="fleet-cluster-chip__count">{cluster.count}</span>
						{#if cluster.errorCount > 0}
							<span class="fleet-cluster-chip__errors"
								>{cluster.errorCount} error{cluster.errorCount === 1 ? '' : 's'}</span
							>
						{/if}
					</button>
				</span>
			{/each}
		</div>
	{/if}
</div>

<style>
	.fleet-topbar {
		display: flex;
		flex-direction: column;
		border-bottom: 1px solid var(--border-default);
		background: var(--surface-2);
	}
	.fleet-topbar__row {
		display: flex;
		align-items: center;
		gap: 14px;
		padding: 11px 16px;
		flex-wrap: wrap;
		position: relative;
	}
	.fleet-topbar__brand {
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 700;
		font-size: 15px;
		letter-spacing: -0.01em;
		color: var(--text-strong);
	}
	.fleet-topbar__field {
		display: flex;
		align-items: center;
		gap: 6px;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 11px;
		color: var(--text-3);
	}
	.fleet-topbar__field select {
		background: var(--surface-card);
		border: 1px solid var(--border-default);
		border-radius: 6px;
		padding: 4px 8px;
		color: var(--text-1);
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 11px;
	}
	.fleet-topbar__project-link {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 22px;
		height: 22px;
		border-radius: 4px;
		color: var(--text-3);
		transition: color 0.12s ease;
	}
	.fleet-topbar__project-link:hover {
		color: var(--interactive-active-fg);
		background: var(--surface-card);
	}
	.fleet-topbar__project-link:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--ring);
	}
	.fleet-topbar__search-label {
		color: var(--text-3);
	}
	.fleet-topbar__search-wrap {
		position: relative;
		display: inline-flex;
		align-items: center;
	}
	.fleet-topbar__search {
		width: 170px;
		background: var(--surface-card);
		border: 1px solid var(--border-default);
		border-radius: 6px;
		padding: 4px 24px 4px 8px;
		color: var(--text-1);
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 11px;
	}
	.fleet-topbar__search::placeholder {
		color: var(--text-faint);
	}
	.fleet-topbar__search:focus-visible {
		outline: none;
		border-color: var(--interactive-active-border);
		box-shadow: 0 0 0 2px var(--ring);
	}
	.fleet-topbar__search-clear {
		position: absolute;
		right: 4px;
		display: inline-flex;
		width: 16px;
		height: 16px;
		align-items: center;
		justify-content: center;
		padding: 0;
		background: transparent;
		border: none;
		border-radius: 3px;
		color: var(--text-faint);
		cursor: pointer;
		transition:
			color 0.14s,
			background 0.14s;
	}
	.fleet-topbar__search-clear:hover {
		color: var(--text-strong);
		background: var(--surface-2);
	}
	.fleet-topbar__search-clear:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--ring);
	}
	.fleet-topbar__no-match,
	.fleet-topbar__match-count {
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 11px;
		color: var(--text-faint);
	}

	.fleet-topbar__view {
		display: inline-flex;
		margin-left: auto;
		border: 1px solid var(--border-default);
		border-radius: 8px;
		overflow: hidden;
		background: var(--surface-card);
	}
	.fleet-topbar__view-btn {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		border: none;
		background: transparent;
		color: var(--text-2);
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-weight: 600;
		font-size: 12.5px;
		padding: 8px 16px;
		cursor: pointer;
		transition:
			color 0.14s,
			background 0.14s;
	}
	.fleet-topbar__view-btn:disabled {
		cursor: not-allowed;
		color: var(--text-faint);
	}
	.fleet-topbar__view-btn + .fleet-topbar__view-btn {
		border-left: 1px solid var(--border-default);
	}
	.fleet-topbar__view-btn:hover:not(:disabled):not(.fleet-topbar__view-btn--active) {
		color: var(--text-strong);
		background: var(--interactive-hover-surface);
	}
	.fleet-topbar__view-btn:focus-visible {
		outline: none;
		box-shadow: inset 0 0 0 2px var(--ring);
	}
	.fleet-topbar__view-btn--active {
		color: var(--interactive-active-fg);
		background: var(--interactive-active-bg, var(--surface-2));
	}

	.fleet-topbar__clusters {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 16px;
		border-top: 1px solid var(--border-subtle);
		flex-wrap: wrap;
	}
	.fleet-topbar__clusters-label {
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: var(--text-caption);
		color: var(--text-3);
		flex: none;
	}
	.fleet-cluster-chip-backing {
		display: inline-flex;
		background: var(--surface-2);
	}
	.fleet-cluster-chip {
		--edge: 4px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		display: inline-flex;
		align-items: center;
		gap: 6px;
		background: var(--surface-card);
		padding: calc(var(--edge) + 4px) 10px;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: var(--text-caption);
		color: var(--text-2);
		cursor: pointer;
		transition: color 0.14s;
	}
	.fleet-cluster-chip:hover:not(.fleet-cluster-chip--active) {
		color: var(--text-strong);
	}
	.fleet-cluster-chip:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--ring);
	}
	.fleet-cluster-chip--active {
		color: var(--text-strong);
	}
	.fleet-cluster-chip__icon {
		display: inline-flex;
	}
	.fleet-cluster-chip__label {
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}
	.fleet-cluster-chip__count {
		color: var(--text-faint);
	}
	.fleet-cluster-chip__errors {
		color: var(--status-danger-fg);
	}
</style>
