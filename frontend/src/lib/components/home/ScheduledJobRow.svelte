<script lang="ts">
	import { Play } from '@lucide/svelte';
	import { resolve } from '$app/paths';
	import type { ScheduledJob } from '$lib/api/scheduledJobs';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { Button } from '$lib/components/ui/button';
	import { StatusMark } from '$lib/components/ui/status-mark';
	import { Switch } from '$lib/components/ui/switch';
	import { fixSegments } from './fixLinks';
	import { offsetMinutes, stampText, windowRange, type Segment } from './jobClock';
	import ScheduledJobLane from './ScheduledJobLane.svelte';
	import { rowView, runsOnAnotherMachine } from './scheduledJobs';

	interface Props {
		job: ScheduledJob;
		machineUuid: string | null;
		now: number;
		timeZone: string;
		open: boolean;
		onToggle: () => void;
		/** A "Run now" press was accepted and the row has not reported the run yet. */
		queued?: boolean;
		/** Why the last press was refused; replaces the next-run line. */
		runError?: string | null;
		onRun?: () => void;
		/** The position the switch was just set to, while the change is being saved. */
		switchPending?: boolean | null;
		/** Why the last switch change was refused; replaces the next-run line. */
		switchError?: string | null;
		onSetEnabled?: (on: boolean) => void;
		onLaneHover?: (segments: Segment[]) => void;
	}

	const {
		job,
		machineUuid,
		now,
		timeZone,
		open,
		onToggle,
		queued = false,
		runError = null,
		onRun,
		switchPending = null,
		switchError = null,
		onSetEnabled,
		onLaneHover
	}: Props = $props();

	const view = $derived(rowView(job, { queued, machineUuid, now, timeZone }));
	const detailId = $derived(`sj-more-${job.id}`);
	const shownOn = $derived(switchPending ?? job.enabled);
	const switchLocked = $derived(job.enabled_key === null || switchPending !== null);
	const switchTip = $derived(
		job.enabled_key === null
			? 'This mike does not say which setting turns the job on.'
			: shownOn
				? 'Runs on its schedule. Turn off to stop scheduled runs.'
				: 'Turn on to run on its schedule again.'
	);
	const showLocal = $derived(offsetMinutes(now, timeZone) !== 0);
	const windowText = $derived(
		job.window && job.window.hours_utc.length > 0
			? windowRange(job.window, now, timeZone, showLocal)
			: '—'
	);
	const remote = $derived(runsOnAnotherMachine(job, machineUuid));
	const startedBy = $derived(
		job.last_trigger === 'schedule' ? 'schedule' : job.last_trigger === 'manual' ? 'hand' : '—'
	);
	const alert = $derived(
		switchError ? `Not changed. ${switchError}` : runError ? `Not started. ${runError}` : null
	);
</script>

<li
	class="sj-row"
	data-slot="scheduled-job"
	data-job-id={job.id}
	data-state={job.state}
	data-status={view.status}
	data-open={open ? '' : undefined}
>
	<div class="sj-name-cell">
		<button
			type="button"
			class="sj-name"
			data-slot="scheduled-job-name"
			data-motion="hover press"
			aria-expanded={open}
			aria-controls={detailId}
			onclick={onToggle}>{job.label}</button
		>
		{#if job.group === 'learning_and_memory'}
			<a class="sj-loop-link" data-slot="scheduled-job-loop-link" href={resolve('/memory')}
				>See it in the loop</a
			>
		{/if}
	</div>
	<span class="sj-status">
		<Tooltip text={view.statusTip}>
			{#snippet child({ props })}
				<span {...props} class="sj-status-tip">
					<StatusMark
						icon={view.icon}
						label={view.word}
						tone={view.tone}
						spin={view.spin}
						data-slot="scheduled-job-state"
						data-status={view.status}
					/>
				</span>
			{/snippet}
		</Tooltip>
	</span>
	<div class="sj-lane-cell">
		<ScheduledJobLane {job} tone={view.laneTone} {now} {timeZone} onHover={onLaneHover} />
	</div>
	<div class="sj-when" data-slot="scheduled-job-when">
		<span class="sj-when-last" data-slot="scheduled-job-last">{view.lastRan}</span>
		{#if alert}
			<span class="sj-when-next sj-error" role="alert" data-slot="scheduled-job-error">{alert}</span
			>
		{:else}
			<span class="sj-when-next" data-slot="scheduled-job-next">{view.next}</span>
		{/if}
	</div>
	<span class="sj-switch" data-slot="scheduled-job-switch-cell">
		<Tooltip text={switchTip}>
			{#snippet child({ props })}
				<span {...props} class="sj-tip">
					<Switch
						bind:checked={() => shownOn, (on) => onSetEnabled?.(on)}
						disabled={switchLocked}
						aria-label="{job.label} on schedule"
						data-slot="scheduled-job-switch"
					/>
				</span>
			{/snippet}
		</Tooltip>
	</span>
	<span class="sj-run" data-slot="scheduled-job-action">
		<Tooltip text={view.runTip}>
			{#snippet child({ props })}
				<span {...props} class="sj-tip">
					<Button
						size="sm"
						variant={view.runVariant}
						disabled={view.runDisabled}
						aria-label="Run {job.label} now"
						aria-busy={view.status === 'running' || view.status === 'queued'}
						data-slot="scheduled-job-run"
						onclick={onRun}
					>
						<Play data-icon="inline-start" aria-hidden="true" />
						Run now
					</Button>
				</span>
			{/snippet}
		</Tooltip>
	</span>
	{#if open}
		<div class="sj-more" id={detailId} data-slot="scheduled-job-more">
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
			{#if job.description}<p class="sj-description">{job.description}</p>{/if}
			<dl class="sj-facts">
				<dt>Schedule</dt>
				<dd>{job.schedule || '—'}</dd>
				<dt>Window</dt>
				<dd data-slot="scheduled-job-window">{windowText}</dd>
				<dt>Started by</dt>
				<dd>{startedBy}</dd>
				<dt>Last started</dt>
				<dd>{stampText(job.last_started_at, timeZone, showLocal)}</dd>
				<dt>Last finished</dt>
				<dd>{stampText(job.last_finished_at, timeZone, showLocal)}</dd>
				<dt>Exit code</dt>
				<dd data-slot="scheduled-job-exit">{job.last_exit_code ?? '—'}</dd>
				<dt>Failures in a row</dt>
				<dd>{job.consecutive_failures}</dd>
				<dt>Runs on</dt>
				<dd>{remote ? `machine ${job.owner_machine_uuid}` : 'this machine'}</dd>
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
</li>

<style>
	.sj-row {
		position: relative;
		display: grid;
		grid-template-columns: 8.5rem minmax(0, 1fr) auto auto;
		align-items: center;
		column-gap: var(--space-3);
		row-gap: var(--space-1);
		margin: 0;
		padding: var(--space-2) 0;
		list-style: none;
	}
	.sj-row::before {
		content: '';
		position: absolute;
		inset-block-start: 0;
		inset-inline: calc(-1 * var(--space-3));
		border-top: var(--hairline) solid var(--border-subtle);
	}
	.sj-row:first-child::before {
		display: none;
	}
	.sj-name-cell {
		grid-column: 1 / 3;
		grid-row: 1;
		justify-self: start;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		min-inline-size: 0;
		margin-inline-start: calc(-1 * var(--space-2));
	}
	.sj-loop-link {
		padding-inline: var(--space-2);
		color: var(--text-3);
		font-size: var(--text-xs);
		text-decoration: underline;
		text-underline-offset: 2px;
	}
	.sj-loop-link:hover {
		color: var(--text-1);
	}
	.sj-loop-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.sj-name {
		min-inline-size: 0;
		padding: var(--space-1) var(--space-2);
		border: none;
		background: transparent;
		color: var(--text-1);
		font: inherit;
		font-size: var(--text-sm);
		font-weight: var(--weight-medium);
		text-align: left;
		overflow-wrap: anywhere;
		cursor: pointer;
	}
	.sj-name:hover,
	.sj-row[data-open] .sj-name {
		background: var(--surface-2);
	}
	.sj-row[data-open] .sj-name {
		color: var(--text-strong);
	}
	.sj-name:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}
	.sj-switch {
		grid-column: 3;
		grid-row: 1;
		display: flex;
		justify-content: center;
	}
	.sj-run {
		grid-column: 4;
		grid-row: 1;
		display: flex;
		justify-content: flex-end;
	}
	.sj-tip {
		display: inline-flex;
	}
	.sj-status {
		grid-column: 1;
		grid-row: 2;
		align-self: start;
	}
	.sj-status-tip {
		display: inline-flex;
	}
	.sj-when {
		grid-column: 2 / -1;
		grid-row: 2;
		display: flex;
		flex-direction: column;
		min-inline-size: 0;
		font-size: var(--text-sm);
		font-variant-numeric: tabular-nums;
		line-height: var(--leading-body);
	}
	.sj-when-last {
		color: var(--text-2);
	}
	.sj-when-next {
		min-block-size: calc(var(--text-sm) * var(--leading-body));
		color: var(--text-3);
	}
	.sj-error {
		color: var(--status-danger-fg);
		overflow-wrap: anywhere;
	}
	.sj-lane-cell {
		grid-column: 1 / -1;
		grid-row: 3;
		min-inline-size: 0;
	}
	.sj-more {
		grid-column: 1 / -1;
		grid-row: 4;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-2) 0 var(--space-3);
		border-top: var(--hairline) solid var(--border-subtle);
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
		border: var(--hairline) solid var(--border-subtle);
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

	@container (min-width: 64rem) {
		.sj-row {
			grid-column: 1 / -1;
			grid-template-columns: subgrid;
			row-gap: 0;
		}
		.sj-name-cell {
			grid-column: 1;
			grid-row: 1;
			justify-self: stretch;
			align-items: stretch;
		}
		.sj-status {
			grid-column: 2;
			grid-row: 1;
			align-self: center;
		}
		.sj-lane-cell {
			grid-column: 3;
			grid-row: 1;
		}
		.sj-when {
			grid-column: 4;
			grid-row: 1;
		}
		.sj-switch {
			grid-column: 5;
			grid-row: 1;
			inline-size: auto;
		}
		.sj-run {
			grid-column: 6;
			grid-row: 1;
			inline-size: auto;
		}
		.sj-more {
			grid-row: 2;
		}
	}
</style>
