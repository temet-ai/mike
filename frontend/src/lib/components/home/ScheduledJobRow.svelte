<script lang="ts">
	import { Loader, Play } from '@lucide/svelte';
	import type { ScheduledJob } from '$lib/api/scheduledJobs';
	import Badge from '$lib/components/shared/Badge.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { Button } from '$lib/components/ui/button';
	import { fixSegments } from './fixLinks';
	import {
		isBusy,
		isRunning,
		isTurnedOff,
		lastRanText,
		nextText,
		runIsTheFix,
		runsOnAnotherMachine,
		stateView
	} from './scheduledJobs';

	interface Props {
		job: ScheduledJob;
		machineUuid: string | null;
		now: number;
		open: boolean;
		onToggle: () => void;
		/** A "Run now" press was accepted and the row has not reported the run yet. */
		queued?: boolean;
		/** Why the last press was refused; replaces the last-ran and next-run cells. */
		runError?: string | null;
		onRun?: () => void;
	}

	const { job, machineUuid, now, open, onToggle, queued = false, runError = null, onRun }: Props =
		$props();

	const view = $derived(stateView(job.state));
	const remote = $derived(runsOnAnotherMachine(job, machineUuid));
	const detailId = $derived(`sj-more-${job.id}`);
	const busy = $derived(isBusy(job, queued));
	const turnedOff = $derived(isTurnedOff(job));
	const runTip = $derived(
		busy
			? isRunning(job)
				? 'Running now.'
				: 'Queued. It starts shortly.'
			: turnedOff
				? 'This job is turned off. Turn it on in Settings, under Scheduled jobs.'
				: 'Runs this job once now. Its schedule does not change.'
	);
</script>

{#snippet line()}
	<span class="sj-label">
		{#if view.needsAttention}
			{job.label}
		{:else}
			<Tooltip text={job.description || job.label}>
				{#snippet child({ props })}
					<span {...props}>{job.label}</span>
				{/snippet}
			</Tooltip>
		{/if}
	</span>
	<span class="sj-state">
		<Badge variant={view.variant} data-slot="scheduled-job-state" data-state={job.state}>
			{view.label}
		</Badge>
	</span>
	{#if remote}
		<span class="sj-when sj-when--remote" data-slot="scheduled-job-remote"
			>runs on another machine</span
		>
	{:else if runError}
		<span class="sj-when sj-when--remote sj-error" role="alert" data-slot="scheduled-job-run-error"
			>Not started. {runError}</span
		>
	{:else}
		<span class="sj-when" data-slot="scheduled-job-last">{lastRanText(job, now)}</span>
		<span class="sj-when" data-slot="scheduled-job-next">{nextText(job, now)}</span>
	{/if}
{/snippet}

{#snippet action()}
	<span class="sj-action" data-slot="scheduled-job-action">
		{#if !remote}
			<Tooltip text={runTip}>
				{#snippet child({ props })}
					<span {...props} class="sj-run-tip">
						<Button
							size="sm"
							variant={runIsTheFix(job) ? 'default' : 'outline'}
							disabled={busy || turnedOff}
							aria-label="Run {job.label} now"
							aria-busy={busy}
							data-slot="scheduled-job-run"
							onclick={onRun}
						>
							{#if busy}
								<Loader data-icon="inline-start" data-motion="wait wait-spin" aria-hidden="true" />
							{:else}
								<Play data-icon="inline-start" aria-hidden="true" />
							{/if}
							Run now
						</Button>
					</span>
				{/snippet}
			</Tooltip>
		{/if}
	</span>
{/snippet}

<li
	class="sj-row"
	data-slot="scheduled-job"
	data-job-id={job.id}
	data-state={job.state}
	style="--sj-border: {view.border};"
>
	<div class="sj-top">
		{#if view.needsAttention}
			<button
				type="button"
				class="sj-line sj-line--button"
				data-motion="hover press"
				aria-expanded={open}
				aria-controls={detailId}
				onclick={onToggle}
			>
				{@render line()}
			</button>
		{:else}
			<div class="sj-line">
				{@render line()}
			</div>
		{/if}
		{@render action()}
	</div>
	{#if view.needsAttention}
		<div class="sj-note" data-slot="scheduled-job-note">
			{#if job.detail}<p class="sj-detail">{job.detail}</p>{/if}
			{#if job.fix}
				<p class="sj-fix">
					{#each fixSegments(job.fix) as segment, i (i)}
						{#if segment.kind === 'link'}
							<!-- eslint-disable svelte/no-navigation-without-resolve -- the fix text is written by the proxy and holds app paths or full URLs, not literal routes -->
							<a
								class="sj-link"
								href={segment.href}
								target={segment.external ? '_blank' : undefined}
								rel={segment.external ? 'noopener noreferrer' : undefined}>{segment.text}</a
							>
							<!-- eslint-enable svelte/no-navigation-without-resolve -->
						{:else}
							{segment.text}
						{/if}
					{/each}
				</p>
			{/if}
		</div>
		{#if open}
			<div class="sj-more" id={detailId} data-slot="scheduled-job-more">
				{#if job.description}<p class="sj-description">{job.description}</p>{/if}
				<dl class="sj-facts">
					<dt>Schedule</dt>
					<dd>{job.schedule || '—'}</dd>
					<dt>Exit code</dt>
					<dd data-slot="scheduled-job-exit">{job.last_exit_code ?? '—'}</dd>
					<dt>Failures in a row</dt>
					<dd>{job.consecutive_failures}</dd>
					<dt>Last started</dt>
					<dd>{job.last_started_at ?? '—'}</dd>
					<dt>Last finished</dt>
					<dd>{job.last_finished_at ?? '—'}</dd>
				</dl>
				<p class="sj-output-title">Output</p>
				{#if job.last_output_tail}
					<!-- A scroll region must be reachable by keyboard to be read. -->
					<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
					<pre
						class="sj-output"
						tabindex="0"
						data-slot="scheduled-job-output">{job.last_output_tail}</pre>
				{:else}
					<p class="sj-description">No output was captured.</p>
				{/if}
			</div>
		{/if}
	{/if}
</li>

<style>
	.sj-row {
		--sj-action-w: 8rem;
		display: flex;
		flex-direction: column;
		margin: 0;
		padding: 0;
		list-style: none;
		background: var(--surface-card);
		border: 1px solid var(--sj-border);
		border-radius: var(--r-tol);
	}
	.sj-top {
		display: grid;
		grid-template-columns: minmax(0, 1fr) var(--sj-action-w);
		align-items: center;
	}
	.sj-action {
		display: flex;
		justify-content: flex-end;
		padding-right: var(--space-3);
	}
	.sj-run-tip {
		display: inline-flex;
	}
	.sj-line {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 6.5rem 14rem 11rem;
		align-items: center;
		gap: var(--space-3);
		padding: var(--space-2) var(--space-3);
		min-height: var(--control-h);
		font-size: var(--text-sm);
		color: var(--text-2);
		text-align: left;
	}
	.sj-line--button {
		width: 100%;
		border: none;
		background: transparent;
		font: inherit;
		font-size: var(--text-sm);
		cursor: pointer;
		border-radius: var(--r-tol);
	}
	.sj-line--button:hover {
		background: var(--surface-2);
	}
	.sj-line--button:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}
	.sj-label {
		min-width: 0;
		color: var(--text-1);
		font-weight: var(--weight-medium);
		overflow-wrap: anywhere;
	}
	.sj-state {
		display: flex;
		align-items: center;
	}
	.sj-when {
		color: var(--text-3);
	}
	.sj-when--remote {
		grid-column: span 2;
	}
	.sj-error {
		color: var(--status-danger-fg);
		overflow-wrap: anywhere;
	}
	.sj-note {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding: 0 var(--space-3) var(--space-2);
	}
	.sj-detail,
	.sj-fix,
	.sj-description,
	.sj-output-title {
		margin: 0;
		font-size: var(--text-sm);
		line-height: var(--leading-body);
		overflow-wrap: anywhere;
	}
	.sj-detail {
		color: var(--text-2);
	}
	.sj-fix {
		color: var(--text-1);
		font-weight: var(--weight-medium);
	}
	.sj-link {
		color: inherit;
		text-decoration: underline;
		text-underline-offset: 2px;
	}
	.sj-more {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-3) var(--space-3);
		border-top: 1px solid var(--border-subtle);
	}
	.sj-description {
		color: var(--text-2);
	}
	.sj-facts {
		display: grid;
		grid-template-columns: max-content minmax(0, 1fr);
		gap: var(--space-1) var(--space-4);
		margin: 0;
		font-size: var(--text-sm);
	}
	.sj-facts dt {
		color: var(--text-3);
	}
	.sj-facts dd {
		margin: 0;
		color: var(--text-1);
		font-family: var(--font-mono);
		overflow-wrap: anywhere;
	}
	.sj-output-title {
		color: var(--text-3);
	}
	.sj-output {
		margin: 0;
		padding: var(--space-2) var(--space-3);
		max-height: 18rem;
		overflow: auto;
		background: var(--surface-bg);
		border: 1px solid var(--border-subtle);
		color: var(--text-1);
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		line-height: var(--leading-body);
		white-space: pre;
	}
	.sj-output:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	@media (max-width: 64rem) {
		.sj-line {
			grid-template-columns: minmax(0, 1fr) 6.5rem;
			row-gap: var(--space-1);
		}
		.sj-when,
		.sj-when--remote {
			grid-column: 1 / -1;
		}
	}
</style>
