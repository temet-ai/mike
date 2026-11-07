<script module lang="ts">
	import type { SessionSummary } from '$lib/types';

	export interface ProjectGroupRow {
		key: string;
		project: string;
		isUngrouped: boolean;
		name: string;
		path: string;
		sessions: SessionSummary[];
		colorClass: string;
	}
</script>

<script lang="ts">
	import { ChevronDown } from '@lucide/svelte';
	import { disclose } from '$lib/components/ui/motion';
	import { resolve } from '$app/paths';
	import PinToggle from '$lib/components/shared/PinToggle.svelte';
	import SessionTableHead from './SessionTableHead.svelte';
	import SessionRow, { type DetailMetrics } from './SessionRow.svelte';
	import type { ProxyEvent } from '$lib/types';

	interface Props {
		group: ProjectGroupRow;
		now: number;
		collapsed: boolean;
		onToggleCollapse: () => void;
		expanded: boolean;
		onToggleShowMore: () => void;
		showMoreInitial?: number;
		detailMetrics: Map<string, DetailMetrics>;
		onRowHoverIntent: (sessionId: string) => void;
		onRowHoverCancel: (sessionId: string) => void;
		latestWsEventForSession: (sessionId: string) => ProxyEvent | null;
		onTogglePin: (session: SessionSummary) => void;
		onToggleProjectPin: () => void;
		isProjectExhausted: boolean;
		isProjectLoadingMore: boolean;
		projectLoadMoreError: string | null;
		onLoadMoreForProject: () => void;
		showColumnHeader?: boolean;
	}

	let {
		group,
		now,
		collapsed,
		onToggleCollapse,
		expanded,
		onToggleShowMore,
		showMoreInitial = 10,
		detailMetrics,
		onRowHoverIntent,
		onRowHoverCancel,
		latestWsEventForSession,
		onTogglePin,
		onToggleProjectPin,
		isProjectExhausted,
		isProjectLoadingMore,
		projectLoadMoreError,
		onLoadMoreForProject,
		showColumnHeader = true
	}: Props = $props();

	const visible = $derived(expanded ? group.sessions : group.sessions.slice(0, showMoreInitial));
	const hiddenCount = $derived(group.sessions.length - visible.length);
	const projectPinned = $derived(group.sessions.every((s) => s.pinned === true));
	const allLoadedRowsShown = $derived(hiddenCount === 0);
</script>

<div class="ssn-project" data-reveal>
	<div
		class="ssn-project-head"
		data-augmented-ui="tl-clip border"
		data-motion="disclose-cut"
		data-collapsed={collapsed}
	>
		<span class="ssn-chip {group.colorClass}" aria-hidden="true"></span>
		{#if !group.isUngrouped && group.path}
			<a
				class="ssn-project-name ssn-project-name--link"
				href={resolve(`projects/${encodeURIComponent(group.path)}` as const)}
			>
				{group.name}
			</a>
		{:else}
			<span class="ssn-project-name">{group.name}</span>
		{/if}
		<button
			type="button"
			class="ssn-project-toggle"
			aria-expanded={!collapsed}
			aria-controls="ssn-project-body-{group.key}"
			aria-label="{collapsed ? 'Expand' : 'Collapse'} {group.name}"
			onclick={onToggleCollapse}
		>
			{#if group.path}
				<span class="ssn-project-path">{group.path}</span>
			{/if}
			<span class="ssn-project-fill"></span>
		</button>
		{#if !group.isUngrouped}
			<span class="ssn-project-pin">
				<PinToggle
					pinned={projectPinned}
					ariaLabel={projectPinned ? 'Unpin project' : 'Pin project'}
					onToggle={onToggleProjectPin}
				/>
			</span>
		{/if}
	</div>

	{#if !collapsed}
		<div class="ssn-table-elev" data-elevation="card" transition:disclose>
			<div class="ssn-table-elev__inner">
				<div
					class="ssn-table"
					id="ssn-project-body-{group.key}"
					data-augmented-ui="bl-clip br-clip border"
				>
					{#if showColumnHeader}
						<div class="ssn-thead-wrap"><SessionTableHead /></div>
					{/if}
					<div class="ssn-tbody">
						{#each visible as s (s.id)}
							<SessionRow
								session={s}
								{now}
								detail={detailMetrics.get(s.id)}
								{latestWsEventForSession}
								onHoverIntent={onRowHoverIntent}
								onHoverCancel={onRowHoverCancel}
								{onTogglePin}
							/>
						{/each}
					</div>
					{#if group.sessions.length > showMoreInitial}
						<button
							type="button"
							class="ssn-showmore"
							data-motion="hover"
							aria-expanded={expanded}
							onclick={onToggleShowMore}
						>
							<ChevronDown
								class="ssn-showmore-caret {expanded ? 'ssn-showmore-caret--open' : ''}"
								data-motion="disclose-marker"
								size={12}
								aria-hidden="true"
							/>
							{expanded
								? 'Show fewer'
								: `Show ${hiddenCount} more session${hiddenCount === 1 ? '' : 's'}`}
						</button>
					{/if}
					{#if !group.isUngrouped && allLoadedRowsShown && !isProjectExhausted}
						<button
							type="button"
							class="ssn-load-older"
							disabled={isProjectLoadingMore}
							onclick={onLoadMoreForProject}
						>
							{isProjectLoadingMore ? 'Loading' : 'Load older sessions'}
						</button>
						{#if projectLoadMoreError}
							<p class="ssn-load-error" role="alert">{projectLoadMoreError}</p>
						{/if}
					{/if}
				</div>
			</div>
		</div>
	{/if}
</div>

<style>
	.ssn-table-elev {
		display: grid;
	}
	.ssn-table-elev__inner {
		min-height: 0;
		overflow: hidden;
	}

	.ssn-project-head {
		display: flex;
		align-items: center;
		gap: 10px;
		min-width: 0;
		padding: 3px 4px 5px 18px;
		color: var(--text-1);
		--motion-cut: 14px;
		--aug-tl: var(--motion-cut);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
	}
	.ssn-project-head[data-collapsed='true'] {
		--motion-cut: 4px;
	}
	.ssn-project-toggle {
		display: flex;
		align-items: center;
		gap: 10px;
		flex: 1 1 auto;
		min-width: 0;
		text-align: left;
		background: transparent;
		border: 0;
		padding: 0;
		font: inherit;
		color: inherit;
		cursor: pointer;
	}
	.ssn-project-toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
		border-radius: 2px;
	}
	.ssn-chip {
		width: 7px;
		height: 7px;
		border-radius: 2px;
		flex-shrink: 0;
		background: var(--text-faint);
	}
	.ssn-chip--1 {
		background: var(--ssn-chip-1);
	}
	.ssn-chip--2 {
		background: var(--ssn-chip-2);
	}
	.ssn-chip--3 {
		background: var(--ssn-chip-3);
	}
	.ssn-chip--4 {
		background: var(--ssn-chip-4);
	}
	.ssn-chip--5 {
		background: var(--ssn-chip-5);
	}
	.ssn-chip--muted {
		background: var(--text-faint);
	}
	.ssn-project-name {
		font-size: 13px;
		font-weight: 600;
		color: var(--text-strong);
		letter-spacing: -0.01em;
		white-space: nowrap;
	}
	.ssn-project-name--link {
		text-decoration: none;
		border-radius: 2px;
	}
	.ssn-project-name--link:hover {
		text-decoration: underline;
		text-decoration-color: var(--text-faint);
	}
	.ssn-project-name--link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.ssn-project-path {
		font-size: var(--text-caption);
		color: var(--text-faint);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		min-width: 0;
	}
	.ssn-project-fill {
		flex: 1 1 auto;
	}
	.ssn-project-pin {
		flex-shrink: 0;
	}

	.ssn-table {
		--edge: 12px;
		--aug-bl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		background: var(--surface-card);
		overflow: hidden;
		box-shadow: inset 0 1px 0 var(--surface-shine);
		container-type: inline-size;
		container-name: session-table;
	}
	.ssn-tbody {
		display: flex;
		flex-direction: column;
	}

	@container session-table (max-width: 799.98px) {
		.ssn-thead-wrap {
			display: none;
		}
	}

	.ssn-showmore,
	.ssn-load-older {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 8px;
		width: 100%;
		height: var(--control-h);
		border: 0;
		border-top: 1px solid var(--border-subtle);
		background: var(--surface-2);
		cursor: pointer;
		font-family: var(--font-display);
		font-size: var(--text-label);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: var(--track-machine, 0.08em);
		color: var(--text-3);
	}
	.ssn-showmore:hover {
		background: var(--surface-3);
		color: var(--text-strong);
	}
	.ssn-showmore:focus-visible,
	.ssn-load-older:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}
	.ssn-showmore :global(.ssn-showmore-caret--open) {
		transform: rotate(180deg);
	}

	.ssn-load-error {
		margin: 4px 16px 0;
		font-size: var(--text-label);
		color: var(--status-danger-fg);
		line-height: 1.5;
	}
	.ssn-load-older:hover:not(:disabled) {
		background: var(--surface-3);
		color: var(--interactive-active-fg);
	}
	.ssn-load-older:disabled {
		opacity: 0.6;
		cursor: default;
	}
</style>
