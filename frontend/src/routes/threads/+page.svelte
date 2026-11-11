<!--
@component
The work threads of one project: the list, and beside it one thread's state, acceptance criteria,
dependencies, linked sessions and history, with editing. mike's database holds the threads, so every
machine with a location for the project shows the same ones.
-->
<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { SvelteURLSearchParams } from 'svelte/reactivity';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import EmptyStateOnboarding from '$lib/components/shared/EmptyStateOnboarding.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Select } from '$lib/components/ui/select';
	import ThreadList from '$lib/components/threads/ThreadList.svelte';
	import ThreadDetailCard from '$lib/components/threads/ThreadDetail.svelte';
	import ThreadEditor from '$lib/components/threads/ThreadEditor.svelte';
	import {
		addThreadEntry,
		checkpointThread,
		patchThread,
		THREAD_STATUSES,
		type ThreadCheckpointInput,
		type ThreadEntryInput,
		type ThreadMetaPatch
	} from '$lib/api/workThreads';
	import type { ThreadsPageData } from './+page';

	const { data }: { data: ThreadsPageData } = $props();

	let editing = $state(false);
	let busy = $state(false);
	let failed = $state<string | null>(null);

	function nameOf(p: { custom_name: string | null; project_path: string }): string {
		return p.custom_name ?? p.project_path.split('/').filter(Boolean).at(-1) ?? p.project_path;
	}

	const projectOptions = $derived(
		(data.projects ?? []).map((p) => ({ value: p.project_uuid, label: nameOf(p) }))
	);
	const statusOptions = [
		{ value: '', label: 'All statuses' },
		...THREAD_STATUSES.map((value) => ({ value }))
	];

	function query(next: { project?: string; status?: string; thread?: string | null }): string {
		const params = new SvelteURLSearchParams();
		const project = next.project ?? data.project;
		const status = next.status ?? data.status;
		const thread = next.thread === undefined ? data.slug : next.thread;
		if (project) params.set('project', project);
		if (status) params.set('status', status);
		if (thread) params.set('thread', thread);
		const text = params.toString();
		return text ? `?${text}` : '';
	}

	function hrefFor(slug: string): string {
		return `${resolve('/threads')}${query({ thread: slug })}`;
	}

	function go(next: { project?: string; status?: string; thread?: string | null }): void {
		editing = false;
		failed = null;
		void goto(`${resolve('/threads')}${query(next)}`, { reset: false });
	}

	async function run(action: () => Promise<unknown>): Promise<void> {
		busy = true;
		failed = null;
		try {
			await action();
			await invalidateAll();
		} catch (e) {
			failed = e instanceof Error ? e.message : String(e);
		} finally {
			busy = false;
		}
	}

	function patch(p: ThreadMetaPatch): void {
		if (!data.project || !data.slug) return;
		const project = data.project;
		const slug = data.slug;
		void run(() => patchThread(project, slug, p));
	}

	function checkpoint(input: ThreadCheckpointInput): void {
		if (!data.project) return;
		const project = data.project;
		void run(() => checkpointThread(project, input));
	}

	function entry(input: ThreadEntryInput): void {
		if (!data.project || !data.slug) return;
		const project = data.project;
		const slug = data.slug;
		void run(() => addThreadEntry(project, slug, input));
	}
</script>

<svelte:head>
	<title>Threads — mike</title>
</svelte:head>

<section class="tp-scroll" aria-label="Threads">
	<div class="tp">
		<PageIntro
			title="Threads"
			level="h1"
			description="The work each project has under way: its state, acceptance criteria, history and the sessions that worked on it."
			learnMoreHref="/help/docs/threads"
			learnMoreLabel="How threads work →"
		>
			{#snippet actions()}
				{#if data.projects && data.projects.length > 0}
					<div class="tp-filters">
						<Select
							label="Project"
							options={projectOptions}
							bind:value={() => data.project ?? '', (v) => go({ project: v, thread: null })}
						/>
						<Select
							label="Status"
							options={statusOptions}
							bind:value={() => data.status, (v) => go({ status: v })}
						/>
					</div>
				{/if}
			{/snippet}
		</PageIntro>

		{#if data.projectsError}
			<LoadErrorBanner
				title="Could not load the projects"
				detail={data.projectsError}
				onRetry={() => void invalidateAll()}
			/>
		{:else if !data.projects || data.projects.length === 0}
			<EmptyStateOnboarding
				headline="No projects yet"
				body="Threads belong to a project. A project appears once a session has worked in it."
				command={null}
				confirmation="Threads written by sessions show up here."
			/>
		{:else if data.listError}
			<LoadErrorBanner
				title="Could not load this project's threads"
				detail={data.listError}
				onRetry={() => void invalidateAll()}
			/>
		{:else if data.list}
			{#if data.list.threads.length === 0}
				<EmptyStateOnboarding
					headline="No threads in this project"
					body="A thread starts when a session checkpoints one with the work_thread_checkpoint tool."
					command={null}
					confirmation="Nothing here needs your attention."
				/>
			{:else}
				<div class="tp-work">
					<ThreadList threads={data.list.threads} selected={data.slug} {hrefFor} />
					<div class="tp-main">
						{#if data.detailError}
							<LoadErrorBanner
								title="Could not load this thread"
								detail={data.detailError}
								onRetry={() => void invalidateAll()}
							/>
						{:else if data.detail}
							{#key `${data.detail.thread.slug}:${data.detail.thread.updated_at}`}
								{#if editing}
									<ThreadEditor
										detail={data.detail}
										{busy}
										error={failed}
										onPatch={patch}
										onCheckpoint={checkpoint}
										onEntry={entry}
										onClose={() => (editing = false)}
									/>
								{/if}
							{/key}
							<ThreadDetailCard detail={data.detail} {hrefFor}>
								{#snippet actions()}
									<Button
										variant="outline"
										aria-pressed={editing}
										onclick={() => (editing = !editing)}>Edit</Button
									>
								{/snippet}
							</ThreadDetailCard>
						{:else}
							<p class="tp-hint">Pick a thread to read its state and history.</p>
						{/if}
					</div>
				</div>
			{/if}
		{/if}
	</div>
</section>

<style>
	.tp-scroll {
		height: 100%;
		min-height: 0;
		overflow-y: auto;
	}
	.tp {
		display: flex;
		flex-direction: column;
		gap: var(--space-5);
		max-width: 1600px;
		padding: var(--space-5) var(--space-6) var(--space-8);
	}
	.tp-filters {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-3);
	}
	.tp-work {
		display: grid;
		grid-template-columns: 24rem minmax(0, 1fr);
		gap: var(--space-5);
		align-items: start;
	}
	.tp-main {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		min-width: 0;
	}
	.tp-hint {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-3);
	}
</style>
