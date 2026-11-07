<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
	import { ApiError } from '$lib/api';
	import type { ScheduledJob, ScheduledJobsResponse } from '$lib/api/scheduledJobs';
	import ScheduledJobsPanel from './ScheduledJobsPanel.svelte';

	const NOW_MS = Date.parse('2026-10-14T12:00:00Z');
	const iso = (offsetMs: number): string => new Date(NOW_MS + offsetMs).toISOString();
	const MIN = 60_000;
	const HOUR = 3_600_000;

	const THIS_MACHINE = 'a1b2c3d4-0000-0000-0000-00000000000a';
	const OTHER_MACHINE = 'f9e8d7c6-0000-0000-0000-00000000000b';

	function job(overrides: Partial<ScheduledJob> & { id: string; label: string }): ScheduledJob {
		return {
			description: 'Runs on a timer.',
			schedule: 'every hour',
			scope: 'this_machine',
			owner_machine_uuid: null,
			enabled: true,
			state: 'ok',
			last_started_at: iso(-12 * MIN),
			last_finished_at: iso(-12 * MIN + 4_000),
			last_exit_code: 0,
			last_output_tail: null,
			next_run_at: iso(48 * MIN),
			running_since: null,
			run_requested_at: null,
			last_trigger: 'schedule',
			consecutive_failures: 0,
			detail: '',
			fix: '',
			...overrides
		};
	}

	const BACKUP = job({
		id: 'capture-db-backup',
		label: 'Capture database backup',
		description: 'Dumps the capture database to the backup folder.',
		schedule: 'every 6 hours',
		last_started_at: iso(-2 * HOUR),
		next_run_at: iso(4 * HOUR)
	});
	const HARVEST = job({
		id: 'memory-harvest',
		label: 'Memory harvest',
		description: 'Collects lessons from finished sessions.',
		schedule: 'every hour'
	});
	const SWEEP = job({
		id: 'cargo-target-sweep',
		label: 'Build folder sweep',
		description: 'Frees disk space from old build folders.',
		schedule: 'daily at 03:00',
		last_started_at: iso(-9 * HOUR),
		next_run_at: iso(15 * HOUR)
	});

	const FAILING = job({
		id: 'learning-promote',
		label: 'Skill promotion',
		description: 'Turns approved lessons into skills in each checkout.',
		schedule: 'every 30 minutes',
		state: 'failing',
		last_exit_code: 2,
		consecutive_failures: 4,
		last_output_tail: Array.from(
			{ length: 40 },
			(_, i) =>
				`2026-10-14T11:${String(10 + i).padStart(2, '0')}:00Z promote: line ${i + 1} of the full run output, kept whole`
		).join('\n'),
		detail: 'The last 4 runs exited with code 2.',
		fix: 'Open /settings and check the checkout path under Learning, then run the job again.'
	});
	const OVERDUE = job({
		id: 'harvest-embeddings',
		label: 'Embedding refresh',
		description: 'Embeds new turns for semantic recall.',
		schedule: 'every hour',
		state: 'overdue',
		last_started_at: iso(-7 * HOUR),
		last_finished_at: iso(-7 * HOUR + 9_000),
		next_run_at: iso(-6 * HOUR),
		detail: 'It last ran 7 hours ago and was due every hour.',
		fix: 'Check that the timer is installed with systemctl --user list-timers.'
	});
	const NEVER_RUN = job({
		id: 'steering-calibration',
		label: 'Steering calibration',
		description: 'Recomputes steering thresholds from labelled turns.',
		schedule: 'daily at 04:00',
		state: 'never_run',
		last_started_at: null,
		last_finished_at: null,
		last_exit_code: null,
		next_run_at: iso(16 * HOUR),
		detail: 'This job has not run yet.',
		fix: 'Run it now, or wait for its first scheduled run.'
	});
	const REMOTE = job({
		id: 'mac-sync',
		label: 'Machine sync',
		description: 'Copies project folders between the two machines.',
		schedule: 'every 15 minutes',
		scope: 'one_machine',
		owner_machine_uuid: OTHER_MACHINE,
		state: 'ok',
		detail: `Machine ${OTHER_MACHINE} owns this job and runs it; this machine does not.`,
		last_started_at: null,
		next_run_at: null
	});
	const RUNNING = job({
		id: 'rollup',
		label: 'Steering rollup',
		description: 'Rolls up steering labels.',
		state: 'running',
		last_started_at: iso(-20_000),
		running_since: iso(-20_000)
	});
	const OFF = job({
		id: 'cargo-sweep',
		label: 'Cargo sweep',
		description: 'Frees disk space from old build folders.',
		enabled: false,
		state: 'off',
		next_run_at: null
	});
	const UNAVAILABLE = job({
		id: 'old-build-sweep',
		label: 'Old build sweep',
		description: 'Frees disk space from old build folders.',
		state: 'unavailable',
		detail:
			'This job is the script scripts/cargo-sweep-all, which mike runs from the directory it was started in. /home/user/.local/state/mike has no scripts folder, so the job does not run on this machine.',
		next_run_at: null
	});
	const BY_HAND = job({
		id: 'by-hand',
		label: 'Index rebuild',
		last_trigger: 'manual',
		last_started_at: iso(-5 * MIN)
	});

	/** A scenario whose job list changes after a press, with the press held open until released. */
	const scenario = {
		phase: 'idle' as 'idle' | 'running',
		release: () => {},
		gate: Promise.resolve()
	};
	function resetScenario(): void {
		scenario.phase = 'idle';
		scenario.gate = new Promise<void>((resolve) => {
			scenario.release = resolve;
		});
	}
	const scenarioLoad = (): Promise<ScheduledJobsResponse> =>
		respond(
			scenario.phase === 'idle'
				? [BACKUP, FAILING]
				: [
						{ ...BACKUP, state: 'running', running_since: iso(-1_000), last_trigger: 'manual' },
						FAILING
					]
		)();
	const scenarioRun = fn(async (id: string): Promise<void> => {
		await scenario.gate;
		if (id === BACKUP.id) scenario.phase = 'running';
	});

	function respond(jobs: ScheduledJob[]): () => Promise<ScheduledJobsResponse> {
		return () => Promise.resolve({ machine_uuid: THIS_MACHINE, machine_name: 'omit', jobs });
	}

	const { Story } = defineMeta({
		title: 'Home/ScheduledJobsPanel',
		component: ScheduledJobsPanel,
		tags: ['autodocs']
	});
</script>

<Story
	name="All healthy"
	args={{ now: NOW_MS, load: respond([BACKUP, HARVEST, SWEEP, RUNNING]) }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await waitFor(() => expect(canvas.getAllByRole('listitem')).toHaveLength(4));
		await expect(canvasElement.querySelector('[aria-expanded]')).toBeNull();
		await expect(canvas.getByText('last ran 2h ago')).toBeInTheDocument();
		await expect(canvas.getByText('next in 4h')).toBeInTheDocument();
		await expect(canvas.getByText('started 20s ago')).toBeInTheDocument();
		const labels = canvas.getAllByRole('listitem').map((li) => li.getAttribute('data-job-id'));
		await expect(labels).toEqual([
			'cargo-target-sweep',
			'capture-db-backup',
			'memory-harvest',
			'rollup'
		]);
	}}
>
	{#snippet template(args)}
		<div class="w-[900px]">
			<ScheduledJobsPanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="One failing with a fix"
	args={{ now: NOW_MS, load: respond([BACKUP, FAILING, HARVEST]) }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await waitFor(() => expect(canvas.getByText('failing')).toBeInTheDocument());
		await expect(canvas.getByText('The last 4 runs exited with code 2.')).toBeInTheDocument();
		await expect(canvas.getByRole('link', { name: '/settings' })).toBeInTheDocument();
		await expect(canvas.queryByText('Exit code')).toBeNull();

		const row = canvasElement.querySelector('[data-job-id="learning-promote"]') as HTMLElement;
		const before = row.getBoundingClientRect().left;
		const toggle = canvas.getByRole('button', { name: /^Skill promotion/ });
		await userEvent.click(toggle);
		await expect(toggle).toHaveAttribute('aria-expanded', 'true');
		await expect(canvas.getByText('Exit code')).toBeInTheDocument();
		const output = canvasElement.querySelector('[data-slot="scheduled-job-output"]') as HTMLElement;
		await expect(output.textContent).toContain('line 40 of the full run output');
		await expect(output.scrollHeight).toBeGreaterThan(output.clientHeight);
		await expect(row.getBoundingClientRect().left).toBe(before);

		await userEvent.click(toggle);
		await expect(toggle).toHaveAttribute('aria-expanded', 'false');
		await expect(canvas.queryByText('Exit code')).toBeNull();
	}}
>
	{#snippet template(args)}
		<div class="w-[900px]">
			<ScheduledJobsPanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="One overdue"
	args={{ now: NOW_MS, load: respond([BACKUP, OVERDUE, HARVEST]) }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await waitFor(() => expect(canvas.getByText('overdue')).toBeInTheDocument());
		await expect(canvas.getByText('last ran 7h ago')).toBeInTheDocument();
		await expect(canvas.getByText('next due now')).toBeInTheDocument();
		await expect(
			canvas.getByText('It last ran 7 hours ago and was due every hour.')
		).toBeInTheDocument();
	}}
>
	{#snippet template(args)}
		<div class="w-[900px]">
			<ScheduledJobsPanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="One never run"
	args={{ now: NOW_MS, load: respond([BACKUP, NEVER_RUN, HARVEST]) }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await waitFor(() => expect(canvas.getByText('never run')).toBeInTheDocument());
		await expect(canvas.getByText('never ran')).toBeInTheDocument();
		await expect(canvas.getByText('next in 16h')).toBeInTheDocument();
		await expect(
			canvas.getByText('Run it now, or wait for its first scheduled run.')
		).toBeInTheDocument();
	}}
>
	{#snippet template(args)}
		<div class="w-[900px]">
			<ScheduledJobsPanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="One running on another machine"
	args={{ now: NOW_MS, load: respond([BACKUP, REMOTE, HARVEST, UNAVAILABLE]) }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await waitFor(() => expect(canvas.getByText('runs on another machine')).toBeInTheDocument());
		await expect(canvas.getAllByText('runs on another machine')).toHaveLength(1);
		const remote = canvasElement.querySelector('[data-job-id="mac-sync"]') as HTMLElement;
		await expect(remote.getAttribute('data-state')).toBe('ok');
		await expect(within(remote).queryByText('off')).toBeNull();
		await expect(canvas.getByText('omit')).toBeInTheDocument();
		const unavailable = canvasElement.querySelector(
			'[data-job-id="old-build-sweep"]'
		) as HTMLElement;
		await expect(unavailable.getAttribute('data-state')).toBe('unavailable');
		await expect(within(unavailable).getByText('this mike has no job scripts')).toBeInTheDocument();
		await expect(within(unavailable).queryByRole('button')).toBeNull();
		await expect(unavailable.getBoundingClientRect().height).toBe(
			remote.getBoundingClientRect().height
		);
		const badge = within(unavailable).getByText('not available');
		const lane = unavailable.querySelector('.sj-state') as HTMLElement;
		await expect(badge.getBoundingClientRect().right).toBeLessThanOrEqual(
			lane.getBoundingClientRect().right + 0.5
		);
	}}
>
	{#snippet template(args)}
		<div class="w-[900px]">
			<ScheduledJobsPanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Run now idle"
	beforeEach={resetScenario}
	args={{
		now: NOW_MS,
		load: scenarioLoad,
		runJob: scenarioRun,
		pollMs: 60_000,
		busyPollMs: 60_000
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const healthy = await canvas.findByRole('button', { name: 'Run Capture database backup now' });
		const failing = canvas.getByRole('button', { name: 'Run Skill promotion now' });
		await expect(healthy).toBeEnabled();
		await expect(healthy).toHaveAttribute('data-variant', 'outline');
		await expect(failing).toBeEnabled();
		await expect(failing).toHaveAttribute('data-variant', 'default');
		await expect(healthy).toHaveTextContent('Run now');

		const idleBox = healthy.getBoundingClientRect();
		await userEvent.click(healthy);
		await expect(scenarioRun).toHaveBeenCalledWith('capture-db-backup');
		await waitFor(() => expect(healthy).toBeDisabled());
		await expect(healthy.querySelector('[data-motion~="wait-spin"]')).not.toBeNull();
		await expect(healthy).toHaveTextContent('Run now');
		const queuedBox = healthy.getBoundingClientRect();
		await expect(queuedBox.left).toBe(idleBox.left);
		await expect(queuedBox.width).toBe(idleBox.width);

		scenario.release();
		await waitFor(() => expect(canvas.getByText('running')).toBeInTheDocument());
		await expect(healthy).toBeDisabled();
		await expect(canvas.getByText(/by hand/)).toBeInTheDocument();
		const runningBox = healthy.getBoundingClientRect();
		await expect(runningBox.left).toBe(idleBox.left);
		await expect(runningBox.width).toBe(idleBox.width);
		await expect(failing).toBeEnabled();
	}}
>
	{#snippet template(args)}
		<div class="w-[900px]">
			<ScheduledJobsPanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Run now running"
	args={{ now: NOW_MS, load: respond([BACKUP, RUNNING]), runJob: fn() }}
	play={async ({ canvasElement, args }) => {
		const canvas = within(canvasElement);
		const button = await canvas.findByRole('button', { name: 'Run Steering rollup now' });
		await expect(button).toBeDisabled();
		await expect(button).toHaveAttribute('aria-busy', 'true');
		await expect(button).toHaveTextContent('Run now');
		await expect(button.querySelector('[data-motion~="wait-spin"]')).not.toBeNull();
		await expect(
			canvas.getByRole('button', { name: 'Run Capture database backup now' })
		).toBeEnabled();
		await expect(args.runJob).not.toHaveBeenCalled();
	}}
>
	{#snippet template(args)}
		<div class="w-[900px]">
			<ScheduledJobsPanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Run now turned off"
	args={{ now: NOW_MS, load: respond([BACKUP, OFF]), runJob: fn() }}
	play={async ({ canvasElement, args }) => {
		const canvas = within(canvasElement);
		const button = await canvas.findByRole('button', { name: 'Run Cargo sweep now' });
		await expect(button).toBeDisabled();
		await expect(button).toHaveTextContent('Run now');
		await expect(button.querySelector('[data-motion~="wait-spin"]')).toBeNull();
		await userEvent.hover(button.parentElement as HTMLElement);
		await expect(
			await within(document.body).findByText(/Turn it on in Settings, under Scheduled jobs/)
		).toBeInTheDocument();
		await expect(args.runJob).not.toHaveBeenCalled();
	}}
>
	{#snippet template(args)}
		<div class="w-[900px]">
			<ScheduledJobsPanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Run now refused"
	args={{
		now: NOW_MS,
		load: respond([BACKUP, OVERDUE]),
		runJob: fn(() =>
			Promise.reject(
				new ApiError(
					'Embedding refresh is already running on this machine.',
					409,
					'/api/scheduled-jobs/harvest-embeddings/run'
				)
			)
		)
	}}
	play={async ({ canvasElement, args }) => {
		const canvas = within(canvasElement);
		const button = await canvas.findByRole('button', { name: 'Run Embedding refresh now' });
		const row = canvasElement.querySelector('[data-job-id="harvest-embeddings"]') as HTMLElement;
		const border = getComputedStyle(row).borderTopColor;
		const before = button.getBoundingClientRect();
		await userEvent.click(button);
		await expect(args.runJob).toHaveBeenCalledWith('harvest-embeddings');
		const alert = await canvas.findByRole('alert');
		await expect(alert).toHaveTextContent(
			'Not started. Embedding refresh is already running on this machine.'
		);
		await expect(button).toBeEnabled();
		await expect(getComputedStyle(row).borderTopColor).toBe(border);
		const after = button.getBoundingClientRect();
		await expect(after.left).toBe(before.left);
		await expect(after.width).toBe(before.width);
		await expect(canvas.queryByText('last ran 7h ago')).toBeNull();
	}}
>
	{#snippet template(args)}
		<div class="w-[900px]">
			<ScheduledJobsPanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Run by hand"
	args={{ now: NOW_MS, load: respond([BACKUP, BY_HAND]), runJob: fn() }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await waitFor(() => expect(canvas.getByText('last ran 5m ago, by hand')).toBeInTheDocument());
		await expect(canvas.getByText('last ran 2h ago')).toBeInTheDocument();
		await expect(canvas.getAllByText(/by hand/)).toHaveLength(1);
		await expect(canvas.getByRole('button', { name: 'Run Index rebuild now' })).toBeEnabled();
	}}
>
	{#snippet template(args)}
		<div class="w-[900px]">
			<ScheduledJobsPanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Route missing"
	args={{
		now: NOW_MS,
		load: () => Promise.reject(new Error('GET /api/scheduled-jobs failed: 404 Not Found'))
	}}
	play={async ({ canvasElement }) => {
		await new Promise((r) => setTimeout(r, 100));
		await expect(canvasElement.querySelector('[data-slot="scheduled-jobs"]')).toBeNull();
		await expect(canvasElement.textContent?.trim()).toBe('');
	}}
>
	{#snippet template(args)}
		<div class="w-[900px]">
			<ScheduledJobsPanel {...args} />
		</div>
	{/snippet}
</Story>
