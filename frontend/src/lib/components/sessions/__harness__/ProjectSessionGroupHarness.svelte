<script lang="ts">
	import ProjectSessionGroup, { type ProjectGroupRow } from '../ProjectSessionGroup.svelte';
	import type { DetailMetrics } from '../sessionRowTypes';
	import type { SessionSummary, ProxyEvent } from '$lib/types';

	interface Props {
		group: ProjectGroupRow;
		now: number;
		detailMetrics: Map<string, DetailMetrics>;
		showMoreInitial?: number;
	}

	let { group: initialGroup, now, detailMetrics, showMoreInitial = 10 }: Props = $props();

	// svelte-ignore state_referenced_locally
	let sessions = $state<SessionSummary[]>(initialGroup.sessions);
	let collapsed = $state(false);
	let expanded = $state(false);
	let projectLoadingMore = $state(false);
	let projectExhausted = $state(false);
	let projectLoadMoreError = $state<string | null>(null);

	const group = $derived<ProjectGroupRow>({ ...initialGroup, sessions });

	function latestWsEventForSession(_sessionId: string): ProxyEvent | null {
		return null;
	}

	function onTogglePin(session: SessionSummary): void {
		sessions = sessions.map((s) =>
			s.id === session.id ? { ...s, pinned: !(s.pinned === true) } : s
		);
	}

	function onToggleProjectPin(): void {
		const allPinned = sessions.every((s) => s.pinned === true);
		sessions = sessions.map((s) => ({ ...s, pinned: !allPinned }));
	}

	async function onLoadMoreForProject(): Promise<void> {
		if (projectExhausted || projectLoadingMore) return;
		projectLoadingMore = true;
		await new Promise((resolve) => setTimeout(resolve, 30));
		sessions = [
			...sessions,
			{
				id: `${initialGroup.key}-older-1`,
				project: initialGroup.isUngrouped ? null : initialGroup.project,
				project_id: initialGroup.isUngrouped ? null : 1,
				cwd: initialGroup.isUngrouped ? null : initialGroup.project,
				project_resolution: initialGroup.isUngrouped ? 'unresolved' : 'git',
				first_seen: '2026-06-10T08:00:00Z',
				last_seen: '2026-06-10T08:10:00Z',
				request_count: 3,
				models: ['claude-opus-4-5'],
				last_status: 'Complete',
				title: 'Older session loaded via "Load older sessions"',
				pinned: false
			}
		];
		projectExhausted = true;
		projectLoadingMore = false;
	}
</script>

<ProjectSessionGroup
	{group}
	{now}
	{collapsed}
	onToggleCollapse={() => (collapsed = !collapsed)}
	{expanded}
	onToggleShowMore={() => (expanded = !expanded)}
	{showMoreInitial}
	{detailMetrics}
	onRowHoverIntent={() => {}}
	onRowHoverCancel={() => {}}
	{latestWsEventForSession}
	{onTogglePin}
	{onToggleProjectPin}
	isProjectExhausted={projectExhausted}
	isProjectLoadingMore={projectLoadingMore}
	{projectLoadMoreError}
	{onLoadMoreForProject}
/>
