<script lang="ts">
	import { resolve } from '$app/paths';
	import { deriveSessionActivity } from '$lib/sessionActivity';
	import { groupByProject, compareByFirstSeenDesc } from '$lib/grouping';
	import ProjectSessionGroup, {
		type ProjectGroupRow
	} from '$lib/components/sessions/ProjectSessionGroup.svelte';
	import type { DetailMetrics } from '$lib/components/sessions/sessionRowTypes';
	import type { SessionListStore } from '$lib/stores/sessions.svelte';
	import type { ProxyEvent, SessionSummary } from '$lib/types';

	interface Props {
		sessionStore: SessionListStore;
		now: number;
		cap?: number;
	}

	const { sessionStore, now, cap = 5 }: Props = $props();

	const UNGROUPED_KEY = '__ungrouped__';
	const PROJECT_COLOR_CLASSES = [
		'ssn-chip--1',
		'ssn-chip--2',
		'ssn-chip--3',
		'ssn-chip--4',
		'ssn-chip--5'
	];

	const noDetail = new Map<string, DetailMetrics>();

	function latestWsEventForSession(sessionId: string): ProxyEvent | null {
		return sessionStore.latestWsEventForSession(sessionId);
	}

	const allActiveSessions = $derived.by((): SessionSummary[] => {
		const all = sessionStore.sessions;
		const active = all.filter((s) => {
			if (s.request_count === 0) return false;
			const wsEvent = latestWsEventForSession(s.id);
			return deriveSessionActivity(s, wsEvent, now).isActive;
		});
		active.sort(compareByFirstSeenDesc);
		return active;
	});

	const activeSessions = $derived.by((): SessionSummary[] => {
		const capped: SessionSummary[] = [];
		for (const s of allActiveSessions) {
			if (capped.length >= cap) break;
			capped.push(s);
		}
		return capped;
	});

	const activeGroups = $derived.by((): ProjectGroupRow[] => {
		return groupByProject(activeSessions).map((group, idx) => {
			const isUngrouped = group.sessions[0]?.project === null;
			const segs = isUngrouped ? [] : group.project.split('/').filter(Boolean);
			const name = isUngrouped ? 'Ungrouped' : (segs[segs.length - 1] ?? group.project);
			return {
				key: isUngrouped ? UNGROUPED_KEY : group.project.toLowerCase(),
				project: group.project,
				isUngrouped,
				name,
				path: isUngrouped ? '' : group.project,
				sessions: group.sessions,
				colorClass: isUngrouped
					? 'ssn-chip--muted'
					: PROJECT_COLOR_CLASSES[idx % PROJECT_COLOR_CLASSES.length]!
			};
		});
	});

	let collapsedKeys = $state(new Set<string>());
</script>

<section
	class="now-band"
	data-slot="now-band"
	aria-label="Sessions"
	data-augmented-ui="tl-clip tr-clip br-clip bl-clip border"
	style="--acc-fg: var(--status-success-fg); --acc-border: var(--status-success-border); --aug-border-bg: linear-gradient(90deg, var(--status-success-fg) 0%, var(--status-success-border) 26%, var(--border-default) 55%); --aug-inlay-bg: var(--status-success-bg);"
>
	<header class="now-head">
		<span class="now-title-backing">
			<h2 class="now-title-heading">
				<a
					class="now-title"
					data-motion="hover"
					href={resolve('/sessions')}
					data-augmented-ui="tr-clip bl-clip border"
					style="--aug-border-bg: var(--status-success-border);"
				>
					Sessions
					<span class="now-title-arrow" data-motion="hover" aria-hidden="true">&rarr;</span>
				</a>
			</h2>
		</span>
	</header>

	{#if activeGroups.length === 0}
		<div class="now-empty" data-slot="now-empty">
			<p>Nothing live right now — sessions appear here the moment traffic flows.</p>
			<div class="now-empty-links">
				<a href={resolve('/sessions')}>All sessions &rarr;</a>
				<a href={resolve('/help/guides/getting-started')}>Getting started</a>
			</div>
		</div>
	{:else}
		<div class="now-groups">
			{#each activeGroups as group (group.key)}
				<ProjectSessionGroup
					{group}
					{now}
					collapsed={collapsedKeys.has(group.key)}
					onToggleCollapse={() => {
						const next = new Set(collapsedKeys);
						if (next.has(group.key)) next.delete(group.key);
						else next.add(group.key);
						collapsedKeys = next;
					}}
					expanded={true}
					onToggleShowMore={() => {}}
					detailMetrics={noDetail}
					onRowHoverIntent={() => {}}
					onRowHoverCancel={() => {}}
					{latestWsEventForSession}
					onTogglePin={(session) => void sessionStore.togglePin(session.id, !session.pinned)}
					onToggleProjectPin={() => {
						const allPinned = group.sessions.every((s) => s.pinned === true);
						void sessionStore.toggleProjectPin(group.project, !allPinned);
					}}
					isProjectExhausted={true}
					isProjectLoadingMore={false}
					projectLoadMoreError={null}
					onLoadMoreForProject={() => {}}
					showColumnHeader={false}
				/>
			{/each}
		</div>
	{/if}
</section>

<style>
	.now-band {
		--edge: 14px;
		--aug-tl: var(--edge);
		--aug-tr: var(--edge);
		--aug-br: var(--edge);
		--aug-bl: var(--edge);
		--aug-border-all: 1.5px;
		--aug-inlay-all: 3px;
		padding: calc(var(--edge) + 4px) calc(var(--edge) + 8px) calc(var(--edge) + 6px)
			calc(var(--edge) + 4px);
		background:
			linear-gradient(105deg, var(--status-success-bg), transparent 34%), var(--surface-bg);
		display: flex;
		flex-direction: column;
		gap: 10px;
	}
	.now-head {
		display: flex;
		flex-direction: column;
	}
	.now-title-backing {
		align-self: flex-start;
		display: inline-flex;
		background: var(--surface-bg);
	}
	.now-title-heading {
		margin: 0;
		display: inline-flex;
	}
	.now-title {
		--aug-border-all: 1px;
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 800;
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--status-success-fg);
		text-decoration: none;
		padding: calc(8px + 4px);
		background:
			linear-gradient(0deg, var(--status-success-bg), var(--status-success-bg)), var(--surface-bg);
		--aug-tl: 0;
		--aug-tr: 8px;
		--aug-br: 0;
		--aug-bl: 8px;
	}
	.now-title:hover,
	.now-title:focus-visible {
		color: var(--text-strong);
	}
	.now-title:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.now-title:hover .now-title-arrow,
	.now-title:focus-visible .now-title-arrow {
		transform: translateX(2px);
	}
	.now-groups {
		display: flex;
		flex-direction: column;
		gap: 0;
		max-width: 1360px;
	}
	.now-empty-links a {
		font-size: var(--text-label);
		color: var(--status-success-fg);
		font-weight: 600;
		text-decoration: none;
	}
	.now-empty-links a:hover {
		text-decoration: underline;
	}
	.now-empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 8px;
		padding: 20px;
		border: 1px solid var(--border-subtle);
		border-radius: 8px;
		background: var(--surface-card);
		text-align: center;
	}
	.now-empty p {
		margin: 0;
		font-size: var(--text-label);
		color: var(--text-3);
	}
	.now-empty-links {
		display: flex;
		gap: 16px;
	}
	.now-empty-links a:not(:first-child) {
		color: var(--text-2);
		font-weight: 500;
	}
</style>
