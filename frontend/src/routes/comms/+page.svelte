<script lang="ts">
	import { onMount, getContext, tick } from 'svelte';
	import { MediaQuery, SvelteMap, SvelteSet } from 'svelte/reactivity';
	import { revealSections } from '$lib/actions/reveal';
	import { Button } from '$lib/components/ui/button';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { ArrowLeft } from '@lucide/svelte';
	import { getFleetProjectsWithCounts } from '$lib/fleet/api';
	import {
		createProjectFor as createProjectIn,
		mergeProject as mergeProjectIn,
		renameProject as renameProjectIn,
		togglePin as togglePinIn,
		type ProjectListAccess
	} from '$lib/fleet/projectMutations';
	import type { FleetProjectWithCounts } from '$lib/fleet/types';
	import { GLOBAL_COMMS_STORE_KEY, createGlobalCommsStore } from '$lib/comms/globalComms.svelte';
	import type { GlobalCommsStore, GlobalResolvedFilter } from '$lib/comms/globalComms.svelte';
	import CommsNewThread from '$lib/comms/CommsNewThread.svelte';
	import CommsProjectList from '$lib/comms/CommsProjectList.svelte';
	import CommsTranscript from '$lib/comms/CommsTranscript.svelte';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import ProjectSettingsDialog from '$lib/components/projects/ProjectSettingsDialog.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import {
		buildThreadIdentityIndex,
		resolveParticipantLabel
	} from '$lib/comms/participantIdentity';
	import { buildCommsProjects } from '$lib/comms/groupThreads';
	import type { ParticipantIdentityDto, PeerSummary } from '$lib/comms/types';

	const commsContextStore = getContext<GlobalCommsStore | undefined>(GLOBAL_COMMS_STORE_KEY);
	const comms: GlobalCommsStore = commsContextStore ?? createGlobalCommsStore();

	let now = $state(Date.now());
	let projectQuery = $state('');
	let expandedProjectPath = $state<string | null>(null);
	let fleetProjects = $state<FleetProjectWithCounts[]>([]);
	let settingsPath = $state<string | null>(null);
	let mounted = $state(false);
	let hydrated = $state(false);
	let readingThread = $state(false);
	let transcriptPanel = $state<HTMLElement | null>(null);
	let listPanel = $state<HTMLElement | null>(null);
	const compactLayout = new MediaQuery('(max-width: 53.75rem)');
	let threadTrigger: HTMLElement | null = null;

	async function selectThread(uuid: string): Promise<void> {
		if (compactLayout.current && document.activeElement instanceof HTMLElement)
			threadTrigger = document.activeElement;
		readingThread = true;
		void comms.selectThread(uuid);
		syncUrl();
		await tick();
		if (compactLayout.current) transcriptPanel?.focus();
	}

	async function showThreads(): Promise<void> {
		readingThread = false;
		if (!compactLayout.current) {
			void comms.selectThread(null);
			syncUrl();
		}
		await tick();
		if (threadTrigger?.isConnected) threadTrigger.focus();
		else listPanel?.focus();
	}

	const derivedPeers = $derived.by((): PeerSummary[] => {
		const identityIndex = new SvelteMap<string, ParticipantIdentityDto>();
		for (const thread of comms.threads) {
			for (const identity of thread.participant_identities ?? []) {
				if (!identityIndex.has(identity.session_uuid))
					identityIndex.set(identity.session_uuid, identity);
			}
		}
		const sessionUuids = new SvelteSet<string>();
		for (const thread of comms.threads) {
			for (const uuid of thread.participants) sessionUuids.add(uuid);
		}
		const peers: PeerSummary[] = Array.from(sessionUuids, (sessionId) => {
			const identity = identityIndex.get(sessionId);
			return {
				sessionId,
				messageCount: 0,
				unconsumedCount: 0,
				identity: {
					role: identity?.role ?? null,
					agentType: identity?.agent_type ?? null,
					state: 'unknown',
					unconsumedMail: 0,
					label: resolveParticipantLabel(sessionId, identityIndex)
				}
			};
		});
		return peers.sort((a, b) =>
			(a.identity.label ?? a.sessionId).localeCompare(b.identity.label ?? b.sessionId)
		);
	});

	function syncUrl(): void {
		if (!hydrated) return;
		const url = new URL(page.url);
		const trimmedQuery = projectQuery.trim();
		if (trimmedQuery.length > 0) url.searchParams.set('project', trimmedQuery);
		else url.searchParams.delete('project');
		if (expandedProjectPath) url.searchParams.set('p', expandedProjectPath);
		else url.searchParams.delete('p');
		if (comms.selectedThreadUuid) url.searchParams.set('thread', comms.selectedThreadUuid);
		else url.searchParams.delete('thread');
		if (comms.filterTag) url.searchParams.set('tag', comms.filterTag);
		else url.searchParams.delete('tag');
		if (comms.filterParticipant) url.searchParams.set('participant', comms.filterParticipant);
		else url.searchParams.delete('participant');
		if (comms.filterResolved !== 'all') url.searchParams.set('resolved', comms.filterResolved);
		else url.searchParams.delete('resolved');
		const next = resolve(`/comms?${url.searchParams.toString()}` as `/comms?${string}`);
		const current = `${page.url.pathname}${page.url.search}`;
		if (next === current) return;
		void goto(next, { replaceState: true, keepFocus: true, noScroll: true });
	}

	function handleFilterTag(tag: string | null): void {
		comms.setFilterTag(tag);
		syncUrl();
	}

	function handleClearParticipant(): void {
		comms.setFilterParticipant(null);
		syncUrl();
	}

	function handleFilterResolved(v: GlobalResolvedFilter): void {
		comms.setFilterResolved(v);
		syncUrl();
	}

	function handleProjectQuery(query: string): void {
		projectQuery = query;
		syncUrl();
	}

	function toggleProject(path: string): void {
		expandedProjectPath = expandedProjectPath === path ? null : path;
		syncUrl();
	}

	async function loadFleetProjects(): Promise<void> {
		try {
			fleetProjects = await getFleetProjectsWithCounts();
		} catch {
			fleetProjects = [];
		}
	}

	function refreshAll(): void {
		void comms.refreshThreads();
		void loadFleetProjects();
	}

	const commsProjects = $derived(buildCommsProjects(comms.threads, fleetProjects));
	const projectList: ProjectListAccess = {
		get: () => fleetProjects,
		set: (next) => (fleetProjects = next)
	};
	const settingsProject = $derived(
		fleetProjects.find((p) => p.project_path === settingsPath) ??
			commsProjects.projects.find((p) => p.project_path === settingsPath)
	);

	function renameProject(path: string, customName: string | null): Promise<void> {
		return renameProjectIn(projectList, path, customName).then(refreshAll);
	}

	async function mergeProject(path: string, intoProjectUuid: string): Promise<void> {
		expandedProjectPath = await mergeProjectIn(projectList, path, intoProjectUuid);
		syncUrl();
		refreshAll();
	}

	async function createProjectFor(path: string, name: string): Promise<void> {
		expandedProjectPath = await createProjectIn(projectList, path, name);
		syncUrl();
		refreshAll();
	}

	async function togglePin(path: string, next: boolean): Promise<void> {
		await togglePinIn(projectList, path, next);
		void comms.refreshThreads();
	}

	onMount(() => {
		const sp = page.url.searchParams;
		const proj = sp.get('project');
		if (proj) projectQuery = proj;
		const expanded = sp.get('p');
		if (expanded) expandedProjectPath = expanded;
		else if (proj) expandedProjectPath = proj;
		const tag = sp.get('tag');
		if (tag) comms.setFilterTag(tag);
		const participant = sp.get('participant');
		if (participant) comms.setFilterParticipant(participant);
		const resolvedParam = sp.get('resolved');
		if (resolvedParam === 'open' || resolvedParam === 'resolved')
			comms.setFilterResolved(resolvedParam);
		hydrated = true;
		const threadParam = sp.get('thread');
		if (threadParam) void selectThread(threadParam);
		void comms.load();
		void loadFleetProjects();
		mounted = true;

		const timer = setInterval(() => (now = Date.now()), 1_000);
		return () => clearInterval(timer);
	});
</script>

<svelte:head>
	<title>Comms — mike</title>
</svelte:head>

{#if settingsProject}
	{#key settingsProject.project_path}
		<ProjectSettingsDialog
			project={settingsProject}
			projects={fleetProjects.length > 0 ? fleetProjects : commsProjects.projects}
			onRenameProject={renameProject}
			onMergeProject={mergeProject}
			onCreateProject={createProjectFor}
			onClose={() => (settingsPath = null)}
		/>
	{/key}
{/if}

<section class="col detail global-comms-page" aria-label="Cross-session comms" use:revealSections>
	<header class="gc-header" data-reveal>
		<PageIntro
			title="Comms"
			level="h1"
			description="Every conversation between your sessions, grouped by project."
		/>
	</header>

	{#if comms.threadsUnavailable}
		<p class="gc-unavailable">Thread comms aren't live yet.</p>
	{:else if comms.threadsError}
		<LoadErrorBanner
			title="Couldn't load threads"
			detail={comms.threadsError}
			onRetry={() => void comms.refreshThreads()}
		/>
	{:else}
		<div
			class="gc-body"
			class:gc-body--reading={readingThread}
			class:gc-body--list-only={comms.selectedThreadUuid === null}
		>
			<div class="gc-body__list" data-reveal bind:this={listPanel} tabindex="-1">
				<CommsProjectList
					projects={commsProjects.projects}
					threadsByPath={commsProjects.threadsByPath}
					tags={comms.tags}
					selectedThreadUuid={comms.selectedThreadUuid}
					filterTag={comms.filterTag}
					filterParticipant={comms.filterParticipant}
					filterResolved={comms.filterResolved}
					{projectQuery}
					{expandedProjectPath}
					openDuration={mounted ? 180 : 0}
					{now}
					loading={!comms.threadsLoaded}
					onSelect={(uuid) => void selectThread(uuid)}
					onFilterTag={handleFilterTag}
					onClearParticipant={handleClearParticipant}
					onFilterResolved={handleFilterResolved}
					onProjectQuery={handleProjectQuery}
					onToggleProject={toggleProject}
					onOpenSettings={(path) => (settingsPath = path)}
					onTogglePin={(path, next) => void togglePin(path, next)}
					onRefresh={refreshAll}
				>
					{#snippet newThread()}
						<CommsNewThread
							sessionId={null}
							project={null}
							peers={derivedPeers}
							onOpened={(uuid) => {
								void comms.refreshThreads();
								void selectThread(uuid);
							}}
						/>
					{/snippet}
				</CommsProjectList>
			</div>
			<div class="gc-body__transcript" data-reveal bind:this={transcriptPanel} tabindex="-1">
				<div class="gc-back">
					<Button variant="outline" onclick={() => void showThreads()}
						><ArrowLeft size={16} aria-hidden="true" /> All threads</Button
					>
				</div>
				<CommsTranscript
					sessionId=""
					readOnly
					subjectLevel="h2"
					thread={comms.selectedThread}
					messages={comms.messages}
					loading={comms.threadLoading}
					error={comms.threadError}
					{now}
					peers={[]}
					replyPhase="idle"
					replyError={null}
					replyNotice={null}
					resolvePhase={comms.resolvePhase}
					onSendReply={() => {}}
					onResolve={() => void comms.resolveSelected()}
					onAck={() => {}}
					onOpenThread={(uuid) => void selectThread(uuid)}
				></CommsTranscript>
			</div>
		</div>
	{/if}
</section>

<style>
	.global-comms-page {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}

	.gc-header {
		padding: var(--space-5) var(--space-5) 0;
		flex-shrink: 0;
	}

	.gc-unavailable {
		margin: 0 var(--space-5);
	}

	.gc-body {
		display: grid;
		grid-template-columns: minmax(22rem, 2fr) minmax(0, 3fr);
		align-items: start;
		gap: var(--space-5);
		padding: 0 var(--space-5) var(--space-5);
	}

	.gc-body__list {
		position: sticky;
		top: 44px;
		max-height: calc(100vh - 44px);
		overflow-y: auto;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}

	.gc-body__transcript {
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.gc-body__transcript > :global(.transcript) {
		flex: 1;
		min-height: 0;
	}

	.gc-back {
		display: flex;
	}

	.gc-body--list-only {
		grid-template-columns: minmax(0, 1fr);
	}

	.gc-body--list-only .gc-body__transcript {
		display: none;
	}

	@media (max-width: 53.75rem) {
		.gc-body {
			grid-template-columns: 1fr;
		}

		.gc-body:not(.gc-body--reading) .gc-body__transcript,
		.gc-body--reading .gc-body__list {
			display: none;
		}
		.gc-body__list {
			position: static;
			max-height: none;
			overflow: visible;
		}
	}
</style>
