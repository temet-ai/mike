<script lang="ts">
	import { onMount } from 'svelte';
	import { SvelteMap, SvelteSet } from 'svelte/reactivity';
	import { ApiError } from '$lib/api';
	import { patchSetting } from '$lib/api/settings';
	import { getScheduledJobs, runScheduledJob } from '$lib/api/scheduledJobs';
	import type { ScheduledJob, ScheduledJobsResponse } from '$lib/api/scheduledJobs';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import type { Segment } from './jobClock';
	import { localClock } from './jobClock';
	import ScheduledJobRow from './ScheduledJobRow.svelte';
	import ScheduledJobsAxis from './ScheduledJobsAxis.svelte';
	import { groupJobs, isBusy, rowView } from './scheduledJobs';

	interface Props {
		now: number;
		/** Fetches the job list. Stories pass fixtures; the page uses the inspect API. */
		load?: () => Promise<ScheduledJobsResponse>;
		pollMs?: number;
		/** Asks the proxy to run one job now. Stories pass fixtures; the page uses the inspect API. */
		runJob?: (id: string) => Promise<void>;
		/** Writes a job's enabled setting. Stories pass fixtures; the page uses the settings API. */
		setEnabled?: (key: string, on: boolean) => Promise<void>;
		/** How often to refresh while a job is queued or running. */
		busyPollMs?: number;
		/** The zone the local clock shows; the browser's by default. */
		timeZone?: string;
	}

	const {
		now,
		load = getScheduledJobs,
		pollMs = 30_000,
		runJob = runScheduledJob,
		setEnabled = async (key, on) => {
			await patchSetting(key, on, 'Scheduled jobs panel on Home');
		},
		busyPollMs = 2_000,
		timeZone = Intl.DateTimeFormat().resolvedOptions().timeZone
	}: Props = $props();

	const NOT_ACCEPTED = 'The proxy did not accept the request.';
	const NOT_SAVED = 'The setting was not saved.';

	let response = $state<ScheduledJobsResponse | null>(null);
	let seeded = false;
	let refreshFailed = $state(false);
	let lastGoodAt = $state<number | null>(null);
	let highlight = $state<Segment[]>([]);
	const openIds = new SvelteSet<string>();
	/** Jobs whose press was accepted, with the last_started_at seen at that moment. */
	const queued = new SvelteMap<string, string | null>();
	const runErrors = new SvelteMap<string, string>();
	/** Jobs whose switch was just moved, with the position it was moved to. */
	const switching = new SvelteMap<string, boolean>();
	const switchErrors = new SvelteMap<string, string>();
	let timer: ReturnType<typeof setTimeout> | null = null;
	let alive = false;

	const jobs = $derived<ScheduledJob[]>(response?.jobs ?? []);
	const groups = $derived(groupJobs(jobs));
	const stale = $derived(
		refreshFailed && lastGoodAt !== null
			? `not refreshed since ${localClock(lastGoodAt, timeZone)}`
			: null
	);

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

	/** Rows that need a look are open on the first load only; a later poll never opens one. */
	function seedOpen(next: ScheduledJobsResponse): void {
		if (seeded) return;
		seeded = true;
		for (const job of next.jobs) {
			const ctx = { queued: false, machineUuid: next.machine_uuid, now, timeZone };
			if (rowView(job, ctx).attention) openIds.add(job.id);
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
			seedOpen(next);
			reconcile(next.jobs);
			lastGoodAt = Date.now();
			refreshFailed = false;
		} catch {
			// The route may not exist yet, or the proxy may be down. A failed first
			// load shows nothing; a failed refresh keeps the rows already on screen.
			if (response !== null) refreshFailed = true;
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

	async function changeEnabled(job: ScheduledJob, on: boolean): Promise<void> {
		const key = job.enabled_key;
		if (key === null || switching.has(job.id)) return;
		switching.set(job.id, on);
		switchErrors.delete(job.id);
		try {
			await setEnabled(key, on);
			await refresh();
		} catch (err) {
			switchErrors.set(job.id, err instanceof Error && err.message ? err.message : NOT_SAVED);
		} finally {
			switching.delete(job.id);
		}
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
			description="What mike runs on a timer, and when. The nightly learning jobs run in order, each after the one before it."
			titleTooltip="Each bar is the window in which a job may start; the line under it is its last run. Run now starts one right away; the switch stops or resumes its schedule."
		>
			{#snippet headerActions()}
				{#if response?.machine_name}
					<span class="sj-machine" data-slot="scheduled-jobs-machine"
						>{response.machine_name}{#if stale}
							<span class="sj-stale" data-slot="scheduled-jobs-stale"> · {stale}</span>{/if}</span
					>
				{/if}
			{/snippet}
			<div class="sj-wrap">
				<div class="sj-list" data-slot="scheduled-jobs-list">
					<ScheduledJobsAxis {now} {timeZone} {highlight} />
					{#each groups as group (group.key)}
						<section class="sj-group" data-slot="scheduled-jobs-group" data-group={group.key}>
							<h4 class="sj-group-title">{group.label}</h4>
							<ul class="sj-rows">
								{#each group.jobs as job (job.id)}
									<ScheduledJobRow
										{job}
										machineUuid={response?.machine_uuid ?? null}
										{now}
										{timeZone}
										open={openIds.has(job.id)}
										onToggle={() => toggle(job.id)}
										queued={queued.has(job.id)}
										runError={runErrors.get(job.id) ?? null}
										onRun={() => void run(job.id)}
										switchPending={switching.get(job.id) ?? null}
										switchError={switchErrors.get(job.id) ?? null}
										onSetEnabled={(on) => void changeEnabled(job, on)}
										onLaneHover={(segments) => (highlight = segments)}
									/>
								{/each}
							</ul>
						</section>
					{/each}
				</div>
			</div>
		</SectionCard>
	</div>
{/if}

<style>
	.sj-machine {
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.sj-stale {
		color: var(--status-warning-fg);
	}
	.sj-wrap {
		container-type: inline-size;
	}
	.sj-list {
		--sj-cols: minmax(10rem, 15rem) 8.5rem minmax(14rem, 1fr) 12rem 2.25rem 7.5rem;
		position: relative;
		padding-inline: var(--space-3);
	}
	.sj-group {
		margin: 0;
		padding: 0;
	}
	.sj-group-title {
		position: relative;
		margin: 0;
		padding: var(--space-3) 0 var(--space-1);
		color: var(--text-3);
		font-size: var(--text-label);
		font-weight: var(--weight-medium);
		letter-spacing: var(--tracking-kpi);
		text-transform: uppercase;
	}
	.sj-group + .sj-group .sj-group-title::before {
		content: '';
		position: absolute;
		inset-block-start: 0;
		inset-inline: calc(-1 * var(--space-3));
		border-top: var(--hairline) solid var(--border-default);
	}
	.sj-rows {
		margin: 0;
		padding: 0;
		list-style: none;
	}

	@container (min-width: 64rem) {
		.sj-list {
			display: grid;
			grid-template-columns: var(--sj-cols);
			column-gap: var(--space-3);
		}
		.sj-group,
		.sj-rows {
			grid-column: 1 / -1;
			display: grid;
			grid-template-columns: subgrid;
		}
		.sj-group-title {
			grid-column: 1 / -1;
		}
	}
</style>
