<script lang="ts" module>
	import type { ProjectSummaryDto } from '$lib/types';

	/** The trigger/row label for a project: its custom name, or the last path segment. */
	export function projectDisplayName(
		project: Pick<ProjectSummaryDto, 'project_path' | 'custom_name'>
	): string {
		if (project.custom_name !== null && project.custom_name.trim().length > 0) {
			return project.custom_name.trim();
		}
		const segments = project.project_path.split('/').filter((s) => s.length > 0);
		return segments[segments.length - 1] ?? project.project_path;
	}

	/** Search is only worth showing once the list is long enough to need it (spec: "searchable if >10"). */
	export const PROJECT_PICKER_SEARCH_THRESHOLD = 10;
</script>

<script lang="ts">
	import { ChevronDown, Globe, SearchX } from '@lucide/svelte';
	import * as Popover from '$lib/components/ui/popover';

	interface Props {
		projects: ProjectSummaryDto[];
		loading: boolean;
		error: string | null;
		/** null = Global. */
		value: string | null;
		onSelect: (projectPath: string | null) => void;
	}

	let { projects, loading, error, value, onSelect }: Props = $props();

	let open = $state(false);
	let search = $state('');

	const currentProject = $derived(
		value === null ? null : (projects.find((p) => p.project_path === value) ?? null)
	);

	const triggerLabel = $derived(
		value === null ? 'Global' : currentProject !== null ? projectDisplayName(currentProject) : value
	);

	const filteredProjects = $derived.by(() => {
		const q = search.trim().toLowerCase();
		if (q.length === 0) return projects;
		return projects.filter(
			(p) =>
				projectDisplayName(p).toLowerCase().includes(q) || p.project_path.toLowerCase().includes(q)
		);
	});

	function handleOpenChange(next: boolean): void {
		open = next;
		if (!next) search = '';
	}

	function pick(projectPath: string | null): void {
		onSelect(projectPath);
		open = false;
		search = '';
	}
</script>

<Popover.Root {open} onOpenChange={handleOpenChange}>
	<Popover.Trigger>
		{#snippet child({ props })}
			<button
				type="button"
				{...props}
				class="wpp-trigger"
				aria-label={`Prompt project: ${triggerLabel}`}
				data-slot="workbench-project-picker-trigger"
			>
				{triggerLabel}
				<ChevronDown size={11} aria-hidden="true" />
			</button>
		{/snippet}
	</Popover.Trigger>
	<Popover.Content class="wpp-popover" side="bottom" align="start">
		{#if error !== null}
			<p class="wpp-error" role="alert">{error}</p>
		{:else}
			{#if projects.length > PROJECT_PICKER_SEARCH_THRESHOLD}
				<div class="wpp-search-wrap">
					<input
						class="wpp-search"
						type="search"
						placeholder="Filter projects…"
						aria-label="Filter projects"
						bind:value={search}
					/>
				</div>
			{/if}

			<div class="wpp-list" role="listbox" aria-label="Prompt project">
				<button
					type="button"
					class="wpp-row"
					class:wpp-row--selected={value === null}
					onclick={() => pick(null)}
					role="option"
					aria-selected={value === null}
				>
					<Globe size={12} aria-hidden="true" class="wpp-row__icon" />
					<span class="wpp-row__name">Global</span>
					{#if value === null}<span class="wpp-row__badge">current</span>{/if}
				</button>
				<div class="wpp-divider"></div>

				{#if loading}
					<p class="wpp-status" aria-live="polite">Loading projects…</p>
				{:else if filteredProjects.length === 0}
					<div class="wpp-empty">
						<SearchX size={14} aria-hidden="true" />
						<span>{search ? `No project matches “${search}”.` : 'No captured projects yet.'}</span>
					</div>
				{:else}
					{#each filteredProjects as project (project.project_path)}
						<button
							type="button"
							class="wpp-row"
							class:wpp-row--selected={value === project.project_path}
							onclick={() => pick(project.project_path)}
							role="option"
							aria-selected={value === project.project_path}
						>
							<span class="wpp-row__name">{projectDisplayName(project)}</span>
							<span class="wpp-row__path">{project.project_path}</span>
							{#if project.session_count !== undefined}
								<span class="wpp-row__count"
									>{project.session_count} session{project.session_count === 1 ? '' : 's'}</span
								>
							{/if}
						</button>
					{/each}
				{/if}
			</div>
		{/if}
	</Popover.Content>
</Popover.Root>

<style>
	.wpp-trigger {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		gap: 5px;
		height: 28px;
		padding: 0 8px;
		font-size: 11px;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		cursor: pointer;
		background: transparent;
		color: var(--text-2);
		border: 1px solid var(--border-default);
		white-space: nowrap;
	}
	.wpp-trigger:hover {
		color: var(--text-strong);
	}
	.wpp-trigger:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}

	:global(.wpp-popover) {
		width: 340px;
		max-width: calc(100dvw - var(--space-6, 24px));
		max-height: min(28rem, var(--bits-popover-content-available-height));
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		padding: 0;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
	}

	.wpp-search-wrap {
		padding: 8px;
		border-bottom: 1px solid var(--border-subtle);
		flex-shrink: 0;
	}
	.wpp-search {
		width: 100%;
		box-sizing: border-box;
		padding: 5px 8px;
		border: 1px solid var(--border-subtle);
		background: var(--surface-2);
		color: var(--text-1);
		font: inherit;
		font-size: 11px;
	}
	.wpp-search:focus-visible {
		outline: none;
		border-color: var(--interactive-focus-ring);
	}

	.wpp-list {
		flex: 1 1 auto;
		min-height: 0;
		overflow-y: auto;
		overscroll-behavior: contain;
	}

	.wpp-divider {
		height: 1px;
		background: var(--border-subtle);
		margin: 4px 0;
		flex-shrink: 0;
	}

	.wpp-row {
		display: flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		min-height: 30px;
		padding: 5px 10px;
		background: none;
		border: none;
		cursor: pointer;
		font: inherit;
		font-size: 11px;
		text-align: left;
		color: var(--text-1);
	}
	.wpp-row:hover {
		background: var(--surface-2);
	}
	.wpp-row:focus-visible {
		outline: none;
		box-shadow: inset 0 0 0 2px var(--interactive-focus-ring);
	}
	.wpp-row--selected {
		background: var(--funk-chip-cyan);
		color: var(--funk-cyan);
	}

	.wpp-row__name {
		flex-shrink: 0;
		font-weight: 600;
	}
	.wpp-row__path {
		flex: 1 1 auto;
		min-width: 0;
		overflow-wrap: anywhere;
		color: var(--text-3);
	}
	.wpp-row--selected .wpp-row__path {
		color: inherit;
		opacity: 0.85;
	}
	.wpp-row__count {
		flex-shrink: 0;
		color: var(--text-3);
		font-size: 10px;
	}
	.wpp-row--selected .wpp-row__count {
		color: inherit;
	}
	.wpp-row__badge {
		margin-left: auto;
		flex-shrink: 0;
		font-size: 9px;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--funk-cyan);
	}

	.wpp-status {
		margin: 0;
		padding: 14px 10px;
		font-size: 11px;
		color: var(--text-3);
	}

	.wpp-empty {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 14px 10px;
		font-size: 11px;
		color: var(--text-3);
	}

	.wpp-error {
		margin: 0;
		padding: 12px 10px;
		font-size: 11px;
		color: var(--status-danger-fg);
	}
</style>
