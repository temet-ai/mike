<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
	import { ApiError } from '$lib/api';
	import type { ScheduledJob, ScheduledJobsResponse } from '$lib/api/scheduledJobs';
	import {
		NOW_DAY,
		NOW_NIGHT,
		OTHER_MACHINE,
		TIME_ZONE,
		iso,
		realJobs,
		realJobsWith,
		response
	} from '../__fixtures__/scheduledJobs';
	import ScheduledJobsPanel from './ScheduledJobsPanel.svelte';

	const MIN = 60_000;

	const respond = (jobs: ScheduledJob[]) => () => Promise.resolve(response(jobs));

	const FAILING: Partial<ScheduledJob> = {
		state: 'failing',
		last_exit_code: 2,
		consecutive_failures: 4,
		last_output_tail: Array.from(
			{ length: 40 },
			(_, i) =>
				`2026-11-13T03:${String(10 + i).padStart(2, '0')}:00Z promote: line ${i + 1} of the full run output, kept whole`
		).join('\n'),
		detail: 'The last 4 runs exited with code 2.',
		fix: 'Open /settings and check the checkout path under Learning, then run the job again.'
	};

	const OVERDUE: Partial<ScheduledJob> = {
		state: 'overdue',
		last_started_at: '2026-11-12T05:15:00Z',
		last_finished_at: '2026-11-12T05:35:00Z',
		next_run_at: '2026-11-13T05:15:00Z',
		detail: 'It was due at 05:15 UTC today and has not run since 05:15 UTC yesterday.',
		fix: 'Check that claude is logged in on this machine, then run it now.'
	};

	const RUNNING_SWEEP: Partial<ScheduledJob> = {
		state: 'running',
		running_since: iso(NOW_DAY - 1_000),
		last_started_at: iso(NOW_DAY - 1_000),
		last_finished_at: null,
		last_trigger: 'manual'
	};

	const OFF_SWEEP: Partial<ScheduledJob> = { enabled: false, state: 'off', next_run_at: null };

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
			realJobsWith({
				learning_promote: FAILING,
				...(scenario.phase === 'running' ? { cargo_sweep: RUNNING_SWEEP } : {})
			})
		)();
	const scenarioRun = fn(async (id: string): Promise<void> => {
		await scenario.gate;
		if (id === 'cargo_sweep') scenario.phase = 'running';
	});

	/** A scenario whose job turns off once the switch is written. */
	const switchScenario = { on: true };
	const switchLoad = (): Promise<ScheduledJobsResponse> =>
		respond(realJobsWith(switchScenario.on ? {} : { cargo_sweep: OFF_SWEEP }))();
	const switchWrite = fn(async (_key: string, on: boolean): Promise<void> => {
		switchScenario.on = on;
	});

	const row = (root: HTMLElement, id: string): HTMLElement =>
		root.querySelector(`[data-job-id="${id}"]`) as HTMLElement;
	const box = (el: Element): DOMRect => el.getBoundingClientRect();
	/** The percentage in a style value such as "61.1%" or "calc(61.1% - 0.6px)". */
	const pct = (el: HTMLElement, prop: 'left' | 'width'): number =>
		parseFloat(/-?[\d.]+(?=%)/.exec(el.style[prop])?.[0] ?? 'NaN');

	/** Gridlines and the now line live inside lanes, so they never cross a name, a time or a control. */
	async function expectGridInsideLanes(root: HTMLElement): Promise<void> {
		const marks = root.querySelectorAll(
			'[data-slot="scheduled-jobs-now"], .sj-gridline, [data-slot="scheduled-jobs-highlight"]'
		);
		await expect(marks.length).toBeGreaterThan(0);
		for (const mark of marks) {
			const lane = mark.closest('[data-slot="scheduled-job-lane"]');
			await expect(lane).not.toBeNull();
			const m = mark.getBoundingClientRect();
			const l = (lane as Element).getBoundingClientRect();
			await expect(m.left).toBeGreaterThanOrEqual(l.left - 0.5);
			await expect(m.right).toBeLessThanOrEqual(l.right + 0.5);
			await expect(m.top).toBeGreaterThanOrEqual(l.top - 0.5);
			await expect(m.bottom).toBeLessThanOrEqual(l.bottom + 0.5);
		}
	}

	const { Story } = defineMeta({
		title: 'Home/ScheduledJobsPanel',
		component: ScheduledJobsPanel,
		tags: ['autodocs']
	});
</script>

<Story
	name="All healthy"
	args={{
		now: NOW_DAY,
		load: respond(realJobs()),
		timeZone: TIME_ZONE,
		runJob: fn(),
		setEnabled: fn()
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await canvas.findByText('Learning and memory');
		const titles = canvas.getAllByRole('heading', { level: 4 }).map((h) => h.textContent);
		await expect(titles).toEqual(['Learning and memory', 'Measurement', 'Builds and tests']);
		const ids = canvas.getAllByRole('listitem').map((li) => li.getAttribute('data-job-id'));
		await expect(ids).toEqual([
			'learning_promote',
			'lesson_relations',
			'learning_audit',
			'lessons_commit',
			'memory_review',
			'compressor_loop',
			'usage_audit',
			'cargo_sweep',
			'nightly_tests'
		]);
		await expect(canvasElement.querySelector('[aria-expanded="true"]')).toBeNull();
		const text = (slot: string): (string | null)[] =>
			[...canvasElement.querySelectorAll(`[data-slot="${slot}"]`)].map((el) => el.textContent);
		await expect(text('axis-utc')).toEqual(['12', '15', '18', '21', '00', '03', '06', '09', '12']);
		await expect(text('axis-local')).toEqual([
			'14',
			'17',
			'20',
			'23',
			'02',
			'05',
			'08',
			'11',
			'14'
		]);
		const now = canvasElement.querySelector('[data-slot="scheduled-jobs-now"]') as HTMLElement;
		await expect(pct(now, 'left')).toBeCloseTo(89.58, 1);
		const audit = row(canvasElement, 'learning_audit').querySelector(
			'[data-slot="scheduled-job-bar"]'
		) as HTMLElement;
		await expect(audit).toHaveTextContent('Mon');
		const relations = row(canvasElement, 'lesson_relations').querySelector(
			'[data-slot="scheduled-job-bar"]'
		) as HTMLElement;
		await expect(pct(relations, 'left')).toBeCloseTo(41.67, 1);
		await expect(pct(relations, 'width')).toBeCloseTo(8.33, 1);
		await expect(canvas.getAllByText('ok')).toHaveLength(9);
		await expectGridInsideLanes(canvasElement);
		const loopLinks = canvas.getAllByRole('link', { name: 'See it in the loop' });
		await expect(loopLinks).toHaveLength(5);
		await expect(loopLinks[0]).toHaveAttribute('href', expect.stringMatching(/\/memory$/));
		await expect(
			within(row(canvasElement, 'cargo_sweep')).queryByRole('link', { name: 'See it in the loop' })
		).toBeNull();
		await expect(
			within(row(canvasElement, 'cargo_sweep')).getByText('ran 3h ago · on schedule')
		).toBeInTheDocument();
	}}
>
	{#snippet template(args)}
		<div class="w-[1200px]">
			<ScheduledJobsPanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Nightly chain running"
	args={{
		now: NOW_NIGHT,
		load: respond(
			realJobsWith(
				{
					learning_promote: {
						last_started_at: '2026-11-13T02:30:00Z',
						last_finished_at: '2026-11-13T02:33:00Z'
					},
					cargo_sweep: {
						last_started_at: '2026-11-13T00:00:00Z',
						last_finished_at: '2026-11-13T00:02:00Z'
					},
					nightly_tests: {
						state: 'running',
						last_started_at: '2026-11-13T02:00:00Z',
						last_finished_at: null,
						running_since: '2026-11-13T02:00:00Z'
					}
				},
				NOW_NIGHT
			)
		),
		timeZone: TIME_ZONE,
		runJob: fn(),
		setEnabled: fn()
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const nightly = await waitFor(() => {
			const el = row(canvasElement, 'nightly_tests');
			if (!el) throw new Error('not loaded');
			return el;
		});
		await expect(nightly.getAttribute('data-status')).toBe('running');
		await expect(within(nightly).getByText('running')).toBeInTheDocument();
		await expect(within(nightly).getByText('started 40m ago · on schedule')).toBeInTheDocument();
		const strip = nightly.querySelector('[data-slot="scheduled-job-strip"]') as HTMLElement;
		const now = canvasElement.querySelector('[data-slot="scheduled-jobs-now"]') as HTMLElement;
		await expect(pct(strip, 'left') + pct(strip, 'width')).toBeCloseTo(pct(now, 'left'), 1);
		const commit = row(canvasElement, 'lessons_commit');
		await expect(within(commit).getByText('ran 1h ago · on schedule')).toBeInTheDocument();
		const run = canvas.getByRole('button', { name: 'Run Nightly full Rust test run now' });
		await expect(run).toBeDisabled();
		await expect(run.querySelector('[data-motion~="wait-spin"]')).toBeNull();
		// No run is later than the clock: no row reads "ran in 2h".
		for (const li of canvasElement.querySelectorAll('[data-slot="scheduled-job"]')) {
			await expect(li.textContent).not.toContain('ran in');
		}
		await expectGridInsideLanes(canvasElement);
	}}
>
	{#snippet template(args)}
		<div class="w-[1200px]">
			<ScheduledJobsPanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="One failing with a fix"
	args={{
		now: NOW_DAY,
		load: respond(realJobsWith({ learning_promote: FAILING })),
		timeZone: TIME_ZONE,
		runJob: fn(),
		setEnabled: fn()
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const toggle = await canvas.findByRole('button', { name: 'Learning promotion' });
		await expect(toggle).toHaveAttribute('aria-expanded', 'true');
		await expect(canvas.getByText('failing')).toBeInTheDocument();
		await expect(canvas.getByText('The last 4 runs exited with code 2.')).toBeInTheDocument();
		await expect(canvas.getByRole('link', { name: '/settings' })).toBeInTheDocument();
		await expect(canvas.getByText('Exit code')).toBeInTheDocument();
		const output = canvasElement.querySelector('[data-slot="scheduled-job-output"]') as HTMLElement;
		await expect(output.textContent).toContain('line 40 of the full run output');
		await expect(output.scrollHeight).toBeGreaterThan(output.clientHeight);

		const failing = row(canvasElement, 'learning_promote');
		const before = box(failing).left;
		await userEvent.click(toggle);
		await expect(toggle).toHaveAttribute('aria-expanded', 'false');
		await expect(canvas.queryByText('Exit code')).toBeNull();
		await expect(box(failing).left).toBe(before);
		await userEvent.click(toggle);
		await expect(canvas.getByText('Exit code')).toBeInTheDocument();
	}}
>
	{#snippet template(args)}
		<div class="w-[1200px]">
			<ScheduledJobsPanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="One overdue"
	args={{
		now: NOW_DAY,
		load: respond(realJobsWith({ memory_review: OVERDUE })),
		timeZone: TIME_ZONE,
		runJob: fn(),
		setEnabled: fn()
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const review = await waitFor(() => {
			const el = row(canvasElement, 'memory_review');
			if (!el) throw new Error('not loaded');
			return el;
		});
		await expect(within(review).getByText('overdue')).toBeInTheDocument();
		await expect(within(review).getByText('ran 1d ago · on schedule')).toBeInTheDocument();
		await expect(within(review).getByText('due now')).toBeInTheDocument();
		await expect(
			canvas.getByText('It was due at 05:15 UTC today and has not run since 05:15 UTC yesterday.')
		).toBeInTheDocument();
		await expect(
			canvas.getByRole('button', { name: 'Run Daily memory review now' })
		).toHaveAttribute('data-variant', 'default');
	}}
>
	{#snippet template(args)}
		<div class="w-[1200px]">
			<ScheduledJobsPanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Run by hand clears overdue"
	args={{
		now: NOW_DAY,
		load: respond(
			realJobsWith({
				memory_review: {
					state: 'ok',
					last_trigger: 'manual',
					last_started_at: iso(NOW_DAY - MIN),
					last_finished_at: iso(NOW_DAY - MIN + 20_000),
					next_run_at: '2026-11-14T05:15:00Z'
				}
			})
		),
		timeZone: TIME_ZONE,
		runJob: fn(),
		setEnabled: fn()
	}}
	play={async ({ canvasElement }) => {
		const review = await waitFor(() => {
			const el = row(canvasElement, 'memory_review');
			if (!el) throw new Error('not loaded');
			return el;
		});
		await expect(within(review).getByText('ran 1m ago · by hand')).toBeInTheDocument();
		await expect(review.textContent?.toLowerCase()).not.toContain('overdue');
		await expect(review.getAttribute('data-status')).toBe('ok');
		const bar = review.querySelector('[data-slot="scheduled-job-bar"]') as HTMLElement;
		const strip = review.querySelector('[data-slot="scheduled-job-strip"]') as HTMLElement;
		// 09:29 UTC on a 12:00 to 12:00 axis.
		await expect(pct(strip, 'left')).toBeCloseTo(89.51, 1);
		await expect(pct(strip, 'left')).toBeGreaterThan(pct(bar, 'left') + pct(bar, 'width'));
	}}
>
	{#snippet template(args)}
		<div class="w-[1200px]">
			<ScheduledJobsPanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="One never run"
	args={{
		now: NOW_DAY,
		load: respond(
			realJobsWith({
				lessons_commit: {
					state: 'never_run',
					last_started_at: null,
					last_finished_at: null,
					last_exit_code: null,
					last_trigger: null,
					detail: 'This job has not run yet.',
					fix: 'Run it now, or wait for its first scheduled run.'
				}
			})
		),
		timeZone: TIME_ZONE,
		runJob: fn(),
		setEnabled: fn()
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const commit = await waitFor(() => {
			const el = row(canvasElement, 'lessons_commit');
			if (!el) throw new Error('not loaded');
			return el;
		});
		await expect(within(commit).getByText('never run')).toBeInTheDocument();
		await expect(within(commit).getByText('never ran')).toBeInTheDocument();
		await expect(within(commit).getByText('next 03:30 · in 16h')).toBeInTheDocument();
		await expect(
			canvas.getByText('Run it now, or wait for its first scheduled run.')
		).toBeInTheDocument();
		await expect(canvas.getAllByText('never ran')).toHaveLength(1);
	}}
>
	{#snippet template(args)}
		<div class="w-[1200px]">
			<ScheduledJobsPanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="One running on another machine"
	args={{
		now: NOW_DAY,
		load: respond(
			realJobsWith({
				cargo_sweep: {
					scope: 'one_machine',
					owner_machine_uuid: OTHER_MACHINE,
					last_started_at: null,
					last_finished_at: null,
					next_run_at: null,
					detail: `Machine ${OTHER_MACHINE} owns this job and runs it; this machine does not.`
				},
				nightly_tests: {
					state: 'unavailable',
					next_run_at: null,
					detail: 'This mike has no scripts folder, so the job does not run on this machine.'
				}
			})
		),
		timeZone: TIME_ZONE,
		runJob: fn(),
		setEnabled: fn()
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const remote = await waitFor(() => {
			const el = row(canvasElement, 'cargo_sweep');
			if (!el) throw new Error('not loaded');
			return el;
		});
		const unavailable = row(canvasElement, 'nightly_tests');
		await expect(remote.getAttribute('data-status')).toBe('elsewhere');
		await expect(within(remote).getByText('elsewhere')).toBeInTheDocument();
		await expect(canvas.getAllByText('runs on another machine')).toHaveLength(1);
		await expect(within(remote).queryByText('never ran')).toBeNull();
		await expect(unavailable.getAttribute('data-status')).toBe('unavailable');
		await expect(within(unavailable).getByText('no scripts')).toBeInTheDocument();
		await expect(within(unavailable).getByText('no scripts folder here')).toBeInTheDocument();
		await expect(canvas.getByText('omit')).toBeInTheDocument();
		for (const name of ['Run Rust build folder sweep now', 'Run Nightly full Rust test run now']) {
			await expect(canvas.getByRole('button', { name })).toBeDisabled();
		}
		await expect(box(unavailable).height).toBe(box(remote).height);
	}}
>
	{#snippet template(args)}
		<div class="w-[1200px]">
			<ScheduledJobsPanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Run now idle"
	beforeEach={resetScenario}
	args={{
		now: NOW_DAY,
		load: scenarioLoad,
		runJob: scenarioRun,
		setEnabled: fn(),
		timeZone: TIME_ZONE,
		pollMs: 60_000,
		busyPollMs: 60_000
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const healthy = await canvas.findByRole('button', { name: 'Run Rust build folder sweep now' });
		const failing = canvas.getByRole('button', { name: 'Run Learning promotion now' });
		await expect(healthy).toBeEnabled();
		await expect(healthy).toHaveAttribute('data-variant', 'outline');
		await expect(failing).toBeEnabled();
		await expect(failing).toHaveAttribute('data-variant', 'default');
		await expect(healthy).toHaveTextContent('Run now');

		const sweep = row(canvasElement, 'cargo_sweep');
		const idleBox = box(healthy);
		const idleRow = box(sweep).height;
		await userEvent.click(healthy);
		await expect(scenarioRun).toHaveBeenCalledWith('cargo_sweep');
		await waitFor(() => expect(healthy).toBeDisabled());
		await expect(within(sweep).getByText('queued')).toBeInTheDocument();
		await expect(healthy.querySelector('[data-motion~="wait-spin"]')).toBeNull();
		await expect(healthy).toHaveTextContent('Run now');
		await expect(box(healthy).left).toBe(idleBox.left);
		await expect(box(healthy).width).toBe(idleBox.width);
		await expect(box(sweep).height).toBe(idleRow);

		scenario.release();
		await waitFor(() => expect(within(sweep).getByText('running')).toBeInTheDocument());
		await expect(healthy).toBeDisabled();
		await expect(within(sweep).getByText(/by hand/)).toBeInTheDocument();
		await expect(box(healthy).left).toBe(idleBox.left);
		await expect(box(healthy).width).toBe(idleBox.width);
		await expect(box(sweep).height).toBe(idleRow);
		await expect(failing).toBeEnabled();
	}}
>
	{#snippet template(args)}
		<div class="w-[1200px]">
			<ScheduledJobsPanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Run now running"
	args={{
		now: NOW_DAY,
		load: respond(realJobsWith({ cargo_sweep: RUNNING_SWEEP })),
		timeZone: TIME_ZONE,
		runJob: fn(),
		setEnabled: fn()
	}}
	play={async ({ canvasElement, args }) => {
		const canvas = within(canvasElement);
		const button = await canvas.findByRole('button', { name: 'Run Rust build folder sweep now' });
		await expect(button).toBeDisabled();
		await expect(button).toHaveAttribute('aria-busy', 'true');
		await expect(button).toHaveTextContent('Run now');
		await expect(button.querySelector('[data-motion~="wait-spin"]')).toBeNull();
		const sweep = row(canvasElement, 'cargo_sweep');
		await expect(
			sweep.querySelector('[data-slot="scheduled-job-state"] [data-motion~="wait-spin"]')
		).not.toBeNull();
		await expect(canvas.getByRole('button', { name: 'Run Learning promotion now' })).toBeEnabled();
		await expect(args.runJob).not.toHaveBeenCalled();
	}}
>
	{#snippet template(args)}
		<div class="w-[1200px]">
			<ScheduledJobsPanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Run now turned off"
	args={{
		now: NOW_DAY,
		load: respond(realJobsWith({ cargo_sweep: OFF_SWEEP })),
		timeZone: TIME_ZONE,
		runJob: fn(),
		setEnabled: fn()
	}}
	play={async ({ canvasElement, args }) => {
		const canvas = within(canvasElement);
		const button = await canvas.findByRole('button', { name: 'Run Rust build folder sweep now' });
		await expect(button).toBeDisabled();
		await expect(button).toHaveTextContent('Run now');
		await expect(button.querySelector('[data-motion~="wait-spin"]')).toBeNull();
		await userEvent.hover(button.parentElement as HTMLElement);
		await expect(
			await within(document.body).findByText(/Turn it on with the switch to run it/)
		).toBeInTheDocument();
		await expect(args.runJob).not.toHaveBeenCalled();
	}}
>
	{#snippet template(args)}
		<div class="w-[1200px]">
			<ScheduledJobsPanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Run now refused"
	args={{
		now: NOW_DAY,
		load: respond(realJobsWith({ memory_review: OVERDUE })),
		timeZone: TIME_ZONE,
		setEnabled: fn(),
		runJob: fn(() =>
			Promise.reject(
				new ApiError(
					'Daily memory review is already running on this machine.',
					409,
					'/api/scheduled-jobs/memory_review/run'
				)
			)
		)
	}}
	play={async ({ canvasElement, args }) => {
		const canvas = within(canvasElement);
		const button = await canvas.findByRole('button', { name: 'Run Daily memory review now' });
		const review = row(canvasElement, 'memory_review');
		const before = box(button);
		await userEvent.click(button);
		await expect(args.runJob).toHaveBeenCalledWith('memory_review');
		const alert = await canvas.findByRole('alert');
		await expect(alert).toHaveTextContent(
			'Not started. Daily memory review is already running on this machine.'
		);
		await expect(button).toBeEnabled();
		await expect(box(button).left).toBe(before.left);
		await expect(box(button).width).toBe(before.width);
		await expect(within(review).getByText('ran 1d ago · on schedule')).toBeInTheDocument();
	}}
>
	{#snippet template(args)}
		<div class="w-[1200px]">
			<ScheduledJobsPanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Switch turns a job off"
	beforeEach={() => {
		switchScenario.on = true;
		switchWrite.mockClear();
	}}
	args={{
		now: NOW_DAY,
		load: switchLoad,
		setEnabled: switchWrite,
		runJob: fn(),
		timeZone: TIME_ZONE,
		pollMs: 60_000
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const toggle = await canvas.findByRole('switch', {
			name: 'Rust build folder sweep on schedule'
		});
		const sweep = row(canvasElement, 'cargo_sweep');
		const run = canvas.getByRole('button', { name: 'Run Rust build folder sweep now' });
		await expect(toggle).toHaveAttribute('aria-checked', 'true');
		await expect(run).toBeEnabled();
		const rowBox = box(sweep);
		const runBox = box(run);
		await userEvent.click(toggle);
		await expect(switchWrite).toHaveBeenCalledWith('cargo_sweep.enabled', false);
		await waitFor(() => expect(sweep.getAttribute('data-status')).toBe('off'));
		await expect(within(sweep).getByText('not scheduled')).toBeInTheDocument();
		await expect(
			sweep.querySelector('[data-slot="scheduled-job-lane"]')?.getAttribute('data-tone')
		).toBe('hollow');
		await expect(run).toBeDisabled();
		await expect(toggle).toHaveAttribute('aria-checked', 'false');
		await expect(box(sweep).left).toBe(rowBox.left);
		await expect(box(sweep).height).toBe(rowBox.height);
		await expect(box(run).left).toBe(runBox.left);
		await expect(box(run).width).toBe(runBox.width);
	}}
>
	{#snippet template(args)}
		<div class="w-[1200px]">
			<ScheduledJobsPanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Switch refused"
	args={{
		now: NOW_DAY,
		load: respond(realJobs()),
		setEnabled: fn(() => Promise.reject(new Error('The setting is locked by the operator.'))),
		runJob: fn(),
		timeZone: TIME_ZONE,
		pollMs: 60_000
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const toggle = await canvas.findByRole('switch', {
			name: 'Rust build folder sweep on schedule'
		});
		const sweep = row(canvasElement, 'cargo_sweep');
		await userEvent.click(toggle);
		await waitFor(() =>
			expect(within(sweep).getByRole('alert')).toHaveTextContent(
				'Not changed. The setting is locked by the operator.'
			)
		);
		await expect(toggle).toHaveAttribute('aria-checked', 'true');
		await expect(toggle).toBeEnabled();
		await expect(sweep.getAttribute('data-status')).toBe('ok');
	}}
>
	{#snippet template(args)}
		<div class="w-[1200px]">
			<ScheduledJobsPanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Phone width"
	args={{
		now: NOW_DAY,
		load: respond(realJobs()),
		timeZone: TIME_ZONE,
		runJob: fn(),
		setEnabled: fn()
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await canvas.findByText('Learning and memory');
		const panel = canvasElement.querySelector('[data-slot="scheduled-jobs"]') as HTMLElement;
		await expect(panel.scrollWidth).toBeLessThanOrEqual(panel.clientWidth);
		await expectGridInsideLanes(canvasElement);
		const visible = [...canvasElement.querySelectorAll('[data-slot="axis-utc"]')].filter(
			(el) => getComputedStyle(el).display !== 'none'
		);
		await expect(visible.map((el) => el.textContent)).toEqual(['12', '18', '00', '06', '12']);
		const axisLeft = box(canvasElement.querySelector('.sj-label-row--utc') as HTMLElement).left;
		for (const lane of canvasElement.querySelectorAll('[data-slot="scheduled-job-lane"]')) {
			await expect(box(lane).left).toBeCloseTo(axisLeft, 0);
		}
	}}
>
	{#snippet template(args)}
		<div class="w-[375px]">
			<ScheduledJobsPanel {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Route missing"
	args={{
		now: NOW_DAY,
		load: () => Promise.reject(new Error('GET /api/scheduled-jobs failed: 404 Not Found'))
	}}
	play={async ({ canvasElement }) => {
		await new Promise((r) => setTimeout(r, 100));
		await expect(canvasElement.querySelector('[data-slot="scheduled-jobs"]')).toBeNull();
		await expect(canvasElement.textContent?.trim()).toBe('');
	}}
>
	{#snippet template(args)}
		<div class="w-[1200px]">
			<ScheduledJobsPanel {...args} />
		</div>
	{/snippet}
</Story>
