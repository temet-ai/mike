<script lang="ts">
	import { onMount } from 'svelte';
	import { SvelteMap, SvelteSet } from 'svelte/reactivity';
	import { ApiError } from '$lib/api';
	import { getScheduledJobs, runScheduledJob } from '$lib/api/scheduledJobs';
	import type { ScheduledJob, ScheduledJobsResponse } from '$lib/api/scheduledJobs';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import ScheduledJobRow from './ScheduledJobRow.svelte';
	import { isBusy, sortJobs } from './scheduledJobs';

	interface Props {
		now: number;
		/** Fetches the job list. Stories pass fixtures; the page uses the inspect API. */
		load?: () => Promise<ScheduledJobsResponse>;
		pollMs?: number;
		/** Asks the proxy to run one job now. Stories pass fixtures; the page uses the inspect API. */
		runJob?: (id: string) => Promise<void>;
		/** How often to refresh while a job is queued or running. */
		busyPollMs?: number;
	}

	const {
		now,
		load = getScheduledJobs,
		pollMs = 30_000,
		runJob = runScheduledJob,
		busyPollMs = 2_000
	}: Props = $props();

	const NOT_ACCEPTED = 'The proxy did not accept the request.';

	let response = $state<ScheduledJobsResponse | null>(null);
	const openIds = new SvelteSet<string>();
	/** Jobs whose press was accepted, with the last_started_at seen at that moment. */
	const queued = new SvelteMap<string, string | null>();
	const runErrors = new SvelteMap<string, string>();
	let timer: ReturnType<typeof setTimeout> | null = null;
	let alive = false;

	const jobs = $derived<ScheduledJob[]>(sortJobs(response?.jobs ?? []));

	function reconcile(list: readonly ScheduledJob[]): void {
		for (const job of list) {
			if (queued.has(job.id)) {
				const before = queued.get(job.id) ?? null;
				const started =
					job.running_since !== null ||
					(job.last_started_at !== null && job.last_started_at !== before);
				if (started) queued.delete(job.id);
			}
			if (runErrors.has(job.id) && isBusy(job, queued.has(job.id))) runErrors.delete(job.id);
		}
	}

	function arm(): void {
		if (timer !== null) clearTimeout(timer);
		if (!alive) return;
		const active = queued.size > 0 || jobs.some((job) => isBusy(job, false));
		timer = setTimeout(() => void refresh(), active ? busyPollMs : pollMs);
	}

	async function refresh(): Promise<void> {
		try {
			const next = await load();
			response = next;
			reconcile(next.jobs);
		} catch {
			// The route may not exist yet, or the proxy may be down. A failed first
			// load shows nothing; a failed refresh keeps the rows already on screen.
		}
		arm();
	}

	onMount(() => {
		alive = true;
		void refresh();
		return () => {
			alive = false;
			if (timer !== null) clearTimeout(timer);
		};
	});

	async function run(id: string): Promise<void> {
		const job = jobs.find((j) => j.id === id);
		if (!job || queued.has(id)) return;
		// Marked before the request returns, so a second press cannot happen.
		queued.set(id, job.last_started_at);
		runErrors.delete(id);
		try {
			await runJob(id);
		} catch (err) {
			queued.delete(id);
			runErrors.set(id, err instanceof ApiError && err.status === 409 ? err.message : NOT_ACCEPTED);
			return;
		}
		await refresh();
	}

	function toggle(id: string): void {
		if (openIds.has(id)) openIds.delete(id);
		else openIds.add(id);
	}
</script>

{#if jobs.length > 0}
	<div data-slot="scheduled-jobs">
		<SectionCard
			title="Scheduled jobs"
			titleTooltip="Every job mike runs on a schedule, when it last ran, and what to do when it did not work. Run now starts one right away."
		>
			{#snippet headerActions()}
				{#if response?.machine_name}
					<span class="sj-machine" data-slot="scheduled-jobs-machine">{response.machine_name}</span>
				{/if}
			{/snippet}
			<ul class="sj-list">
				{#each jobs as job (job.id)}
					<ScheduledJobRow
						{job}
						machineUuid={response?.machine_uuid ?? null}
						{now}
						open={openIds.has(job.id)}
						onToggle={() => toggle(job.id)}
						queued={queued.has(job.id)}
						runError={runErrors.get(job.id) ?? null}
						onRun={() => void run(job.id)}
					/>
				{/each}
			</ul>
		</SectionCard>
	</div>
{/if}

<style>
	.sj-machine {
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.sj-list {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		margin: 0;
		padding: 0;
		list-style: none;
	}
</style>
