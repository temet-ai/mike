<script lang="ts">
	import { untrack } from 'svelte';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import InstrumentReadout from '$lib/components/shared/InstrumentReadout.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Checkbox } from '$lib/components/ui/checkbox';
	import { Select } from '$lib/components/ui/select';
	import { ShareBar } from '$lib/components/ui/share-bar';
	import { getTimeSpent, type TimeSpentParams } from '$lib/api';
	import { formatTimeSpent } from '$lib/time';
	import { buildLabel, kindLabel, kindRunsShellCommands } from '$lib/timeSpent';
	import type { TimeSpentCommandDto, TimeSpentDto, TimeSpentKindDto } from '$lib/types';

	interface ProjectOption {
		value: string;
		label: string;
	}

	interface Props {
		scope: 'list' | 'session' | 'project';
		sessionId?: string;
		projectPath?: string;
		sessionFrom?: string;
		projects?: ProjectOption[];
		grouped?: boolean;
	}

	let {
		scope,
		sessionId,
		sessionFrom,
		projectPath,
		projects = [],
		grouped = false
	}: Props = $props();

	const uid = $props.id();
	const PAGE_SIZE = 20;
	const PERIOD_OPTIONS = [
		{ value: '1', label: 'Last day' },
		{ value: '7', label: 'Last 7 days' },
		{ value: '14', label: 'Last 14 days' },
		{ value: '30', label: 'Last 30 days' }
	];

	let period = $state('14');
	let project = $state('');
	let includeSubagents = $state(false);
	let selectedKind = $state<string | null>(null);

	let data = $state<TimeSpentDto | null>(null);
	let commands = $state<TimeSpentCommandDto[]>([]);
	let hasMore = $state(false);
	let loading = $state(true);
	let moreLoading = $state(false);
	let error = $state<string | null>(null);
	let announcement = $state('');
	let retryTick = $state(0);
	let generation = 0;

	const isList = $derived(scope === 'list');
	const hasPeriod = $derived(scope !== 'session');
	const sectionKey = $derived(
		scope === 'list'
			? 'sessions:time-spent'
			: scope === 'project'
				? `project:${projectPath ?? ''}:time-spent-open`
				: `session:${sessionId ?? ''}:time-spent-open`
	);
	const projectOptions = $derived<ProjectOption[]>([
		{ value: '', label: 'All projects' },
		...projects
	]);
	const periodDays = $derived(Number(period));

	function currentParams(): TimeSpentParams | null {
		const base: TimeSpentParams = {
			include_subagents: includeSubagents,
			kind: selectedKind ?? undefined
		};
		if (scope === 'list') {
			return { ...base, days: periodDays, project: project || undefined };
		}
		if (scope === 'project') {
			if (!projectPath) return null;
			return { ...base, days: periodDays, project_path: projectPath };
		}
		if (!sessionId || !sessionFrom) return null;
		return { ...base, session_id: sessionId, from: sessionFrom };
	}

	async function load(params: TimeSpentParams): Promise<void> {
		const mine = ++generation;
		loading = true;
		error = null;
		try {
			const result = await getTimeSpent({ ...params, limit: PAGE_SIZE });
			if (mine !== generation) return;
			const firstLoad = data === null;
			data = result;
			commands = result.slow_commands;
			hasMore = result.slow_commands.length >= PAGE_SIZE;
			announcement = '';
			if (!firstLoad) {
				queueMicrotask(() => {
					if (mine === generation) announcement = 'Updated.';
				});
			}
		} catch (err) {
			if (mine !== generation) return;
			error = err instanceof Error ? err.message : String(err);
		} finally {
			if (mine === generation) loading = false;
		}
	}

	async function loadMore(): Promise<void> {
		const params = currentParams();
		if (!params || moreLoading) return;
		const mine = generation;
		moreLoading = true;
		try {
			const result = await getTimeSpent({
				...params,
				limit: PAGE_SIZE,
				offset: commands.length
			});
			if (mine !== generation) return;
			commands = [...commands, ...result.slow_commands];
			hasMore = result.slow_commands.length >= PAGE_SIZE;
		} catch (err) {
			if (mine !== generation) return;
			error = err instanceof Error ? err.message : String(err);
		} finally {
			moreLoading = false;
		}
	}

	$effect(() => {
		const params = currentParams();
		void retryTick;
		if (!params) return;
		untrack(() => void load(params));
	});

	function unbroken(seconds: number): string {
		return formatTimeSpent(seconds).replaceAll(' ', ' ');
	}

	function toggleKind(kind: string): void {
		selectedKind = selectedKind === kind ? null : kind;
	}

	const answered = $derived(data !== null && data.totals.answers > 0);
	const refetching = $derived(loading && data !== null);
	const kinds = $derived<TimeSpentKindDto[]>(
		data ? [...data.by_kind].sort((a, b) => b.wait_seconds - a.wait_seconds) : []
	);
	const maxWait = $derived(kinds.reduce((m, k) => Math.max(m, k.wait_seconds), 0));
	const modelShare = $derived.by(() => {
		if (!data) return 0;
		const all = data.totals.model_seconds + data.totals.wait_seconds;
		return all > 0 ? (data.totals.wait_seconds / all) * 100 : 0;
	});
	const builds = $derived(data ? [...data.builds_running_at_once].sort((a, b) => a.sort_key - b.sort_key) : []);
	const alone = $derived(builds.find((b) => b.running_at_once === 'none'));
	const worst = $derived(builds.find((b) => b.running_at_once === '6 or more'));
	const maxBuild = $derived(builds.reduce((m, b) => Math.max(m, b.p50_seconds), 0));
	const slowdown = $derived(alone && worst && alone.p50_seconds > 0 ? worst.p50_seconds / alone.p50_seconds : 0);
	const leftOut = $derived(kinds.filter((k) => k.answers_over_an_hour > 0));
	const leftOutTotal = $derived(leftOut.reduce((n, k) => n + k.answers_over_an_hour, 0));

	const headerLine = $derived.by(() => {
		if (error) return "Couldn't load";
		if (!data) return 'Loading';
		if (data.totals.answers === 0) return 'No answers';
		const core = `${formatTimeSpent(data.totals.model_seconds)} model · ${formatTimeSpent(data.totals.wait_seconds)} waiting`;
		if (!hasPeriod) return core;
		return `${core} · last ${periodDays === 1 ? 'day' : `${periodDays} days`}`;
	});

	const where = $derived(hasPeriod ? 'in this period' : 'in this session');
	const commandsHeading = $derived(
		selectedKind ? `Slowest commands · ${kindLabel(selectedKind)}` : 'Slowest commands'
	);
	const columnCount = $derived(hasPeriod ? 6 : 5);
</script>

<SectionCard
	title="Where the time went"
	collapsible
	bordered={!grouped}
	{sectionKey}
	defaultCollapsed
	titleTooltip="How long the model spent answering, and how long sessions then waited on the tools each answer called."
>
	{#snippet headerActions()}
		<span class="tsp-meta" data-slot="time-spent-summary">{headerLine}</span>
	{/snippet}
	{#snippet children()}
		<div class="tsp" data-slot="time-spent-panel">
			<span class="tsp-live" role="status" aria-live="polite">{announcement}</span>

			<div class="tsp-controls">
				{#if isList}
					<Select label="Project" bind:value={project} options={projectOptions} />
				{/if}
				{#if hasPeriod}
					<Select label="Period" bind:value={period} options={PERIOD_OPTIONS} />
				{/if}
				<Checkbox bind:checked={includeSubagents} label="Include subagents" />
			</div>

			{#if error}
				<LoadErrorBanner
					title="Couldn't load where the time went"
					detail={error}
					onRetry={() => (retryTick += 1)}
				/>
			{:else}
				<div class="tsp-body" class:tsp-body--busy={refetching} aria-busy={refetching}>
					{#if data && !answered}
						<p class="tsp-caption">
							{hasPeriod
								? 'No answers that called a tool in this period.'
								: 'This session has no answers that called a tool yet.'}
						</p>
					{:else}
						<div class="tsp-two">
							<section class="tsp-split" aria-labelledby="{uid}-split">
								<h2 class="machine tsp-hd" id="{uid}-split">Model against waiting</h2>
								<div class="tsp-readouts">
									<InstrumentReadout
										label="Model answering"
										value={data ? formatTimeSpent(data.totals.model_seconds) : '—'}
										accentToken="--signal-live"
									/>
									<InstrumentReadout
										label="Waiting on tools"
										value={data ? formatTimeSpent(data.totals.wait_seconds) : '—'}
										accentToken="--signal-info"
									/>
									<InstrumentReadout
										label="Answers"
										value={data ? data.totals.answers.toLocaleString('en-US') : '—'}
										accentToken="--text-3"
									/>
								</div>
								{#if data}
									<ShareBar
										size="split"
										segments={[
											{ value: data.totals.model_seconds, tone: 'live' },
											{ value: data.totals.wait_seconds, tone: 'info' }
										]}
									/>
									<p class="tsp-caption">
										{Math.round(modelShare)}% of the time went to waiting on tools.
									</p>
								{/if}
							</section>

							<section class="tsp-builds" aria-labelledby="{uid}-builds">
								<h2 class="machine tsp-hd" id="{uid}-builds">
									Builds and test runs while other sessions build
								</h2>
								{#if !data}
									{@render skeleton(4)}
								{:else if builds.length === 0}
									<p class="tsp-caption">
										No builds or test runs {where}.
									</p>
								{:else}
									{#if alone && worst}
										<p class="tsp-lead">
											{#if slowdown >= 2}
												A build or test run took <strong>{Math.round(slowdown)} times as long</strong>
												with 6 or more others running: {unbroken(worst.p50_seconds)} against
												{unbroken(alone.p50_seconds)} with none.
											{:else}
												Builds and test runs took about as long with others running as with none.
											{/if}
										</p>
									{/if}
									<div class="tsp-build-head">
										<span>Others running</span>
										<span></span>
										<span class="tsp-num">Typical</span>
										<span class="tsp-num">Runs</span>
									</div>
									<ul class="tsp-list">
										{#each builds as row (row.running_at_once)}
											{@const slow = alone !== undefined && row.p50_seconds >= alone.p50_seconds * 2}
											<li class="tsp-build">
												<span class="tsp-label">{buildLabel(row.running_at_once)}</span>
												<ShareBar
													segments={[{ value: row.p50_seconds, tone: slow ? 'warn' : 'info' }]}
													max={maxBuild}
												/>
												<span class="tsp-num tsp-strong">{formatTimeSpent(row.p50_seconds)}</span>
												<span class="tsp-num">{row.calls.toLocaleString('en-US')}</span>
											</li>
										{/each}
									</ul>
								{/if}
							</section>
						</div>

						<section class="tsp-kinds" aria-labelledby="{uid}-kinds">
							<h2 class="machine tsp-hd" id="{uid}-kinds">Where the waiting went</h2>
							<p class="tsp-caption">Pick a kind of command to list its slowest commands.</p>
							<div class="tsp-kind-head">
								<span>Kind</span>
								<span></span>
								<span class="tsp-num">Waited</span>
								<span class="tsp-num">Answers</span>
								<span class="tsp-num">
									<Tooltip text="Half the answers waited less than this.">Typical</Tooltip>
								</span>
								<span class="tsp-num">
									<Tooltip text="One answer in ten waited longer than this.">1 in 10 over</Tooltip>
								</span>
							</div>
							{#if !data}
								{@render skeleton(4)}
							{:else}
								<ul class="tsp-list">
									{#each kinds as row (row.kind)}
										{@const pressed = selectedKind === row.kind}
										<li>
											{#if kindRunsShellCommands(row.kind)}
												<button
													type="button"
													class="tsp-kind tsp-kind--pick"
													aria-pressed={pressed}
													onclick={() => toggleKind(row.kind)}
												>
													{@render kindCells(row, pressed)}
												</button>
											{:else}
												<div class="tsp-kind">{@render kindCells(row, false)}</div>
											{/if}
										</li>
									{/each}
								</ul>
								{#if leftOutTotal > 0}
									<p class="tsp-caption">
										Left out of the sums: {leftOutTotal} waits over an hour ({leftOut
											.map((k) => `${kindLabel(k.kind)} ${k.answers_over_an_hour}`)
											.join(', ')}).
									</p>
								{/if}
							{/if}
						</section>

						<section class="tsp-commands" aria-labelledby="{uid}-commands">
							<div class="tsp-commands-top">
								<h2 class="machine tsp-hd" id="{uid}-commands">{commandsHeading}</h2>
								<span class="tsp-show-all" class:tsp-show-all--off={selectedKind === null}>
									<Button
										variant="ghost"
										size="sm"
										disabled={selectedKind === null}
										onclick={() => (selectedKind = null)}>Show every kind</Button
									>
								</span>
							</div>
							{#if !data}
								{@render skeleton(2)}
							{:else}
								<table class="tsp-table">
									<thead>
										<tr>
											<th scope="col">Command</th>
											<th scope="col">Kind</th>
											<th scope="col" class="tsp-num">Runs</th>
											<th scope="col" class="tsp-num">Total</th>
											<th scope="col" class="tsp-num">
												<Tooltip text="Half the runs took less than this.">Typical</Tooltip>
											</th>
											{#if hasPeriod}<th scope="col" class="tsp-num">Sessions</th>{/if}
										</tr>
									</thead>
									<tbody>
										{#each commands as row, i (i)}
											<tr>
												<td class="tsp-cmd"><code>{row.command}</code></td>
												<td data-label="Kind">{kindLabel(row.kind)}</td>
												<td class="tsp-num" data-label="Runs">{row.calls.toLocaleString('en-US')}</td>
												<td class="tsp-num tsp-strong" data-label="Total">
													{formatTimeSpent(row.seconds)}
												</td>
												<td class="tsp-num" data-label="Typical">{formatTimeSpent(row.p50_seconds)}</td>
												{#if hasPeriod}
													<td class="tsp-num" data-label="Sessions">{row.sessions}</td>
												{/if}
											</tr>
										{:else}
											<tr>
												<td colspan={columnCount} class="tsp-empty">
													No commands of this kind {where}.
												</td>
											</tr>
										{/each}
									</tbody>
								</table>
								{#if hasMore}
									<div class="tsp-more">
										<Button variant="ghost" size="sm" disabled={moreLoading} onclick={loadMore}>
											Show 20 more
										</Button>
									</div>
								{/if}
							{/if}
						</section>
					{/if}

					{#if data}
						<section class="tsp-notes" aria-labelledby="{uid}-notes">
							<h2 class="machine tsp-hd" id="{uid}-notes">How this is measured</h2>
							<ul>
								{#each data.notes as note, i (i)}
									<li>{note}</li>
								{/each}
							</ul>
						</section>
					{/if}
				</div>
			{/if}
		</div>
	{/snippet}
</SectionCard>

{#snippet skeleton(rows: number)}
	<div class="tsp-skeleton" aria-hidden="true">
		{#each Array.from({ length: rows }, (_, i) => i) as i (i)}
			<div class="tsp-skel-block" data-motion="wait"></div>
		{/each}
	</div>
{/snippet}

{#snippet kindCells(row: TimeSpentKindDto, pressed: boolean)}
	<span class="tsp-label" class:tsp-label--on={pressed}>{kindLabel(row.kind)}</span>
	<ShareBar
		segments={[{ value: row.wait_seconds, tone: pressed ? 'active' : 'info' }]}
		max={maxWait}
	/>
	<span class="tsp-num tsp-strong">{formatTimeSpent(row.wait_seconds)}</span>
	<span class="tsp-stats">
		<span class="tsp-num"
			>{row.answers.toLocaleString('en-US')}<span class="tsp-inline-label"
				>{' answers'}</span
			></span
		>
		<span class="tsp-num"
			><span class="tsp-inline-label">{'typical '}</span>{formatTimeSpent(row.p50_wait_seconds)}</span
		>
		<span class="tsp-num"
			><span class="tsp-inline-label">{'1 in 10 over '}</span>{formatTimeSpent(
				row.p90_wait_seconds
			)}</span
		>
	</span>
{/snippet}

<style>
	.tsp-meta {
		font-size: var(--text-xs);
		color: var(--text-3);
		font-variant-numeric: tabular-nums;
	}
	.tsp {
		container-type: inline-size;
		container-name: time-spent;
		display: flex;
		flex-direction: column;
		gap: var(--space-5);
	}
	.tsp-live {
		position: absolute;
		width: 1px;
		height: 1px;
		margin: -1px;
		clip-path: inset(50%);
		white-space: nowrap;
	}
	.tsp-controls {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-end;
		gap: var(--space-4);
	}
	.tsp-controls > :global([data-slot='select']) {
		width: 14rem;
		max-width: 100%;
	}
	.tsp-body {
		display: flex;
		flex-direction: column;
		gap: var(--space-5);
	}
	.tsp-body--busy {
		opacity: 0.6;
	}
	.tsp-hd {
		margin: 0 0 var(--space-2);
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.tsp-caption {
		margin: var(--space-2) 0 0;
		font-size: var(--text-sm);
		color: var(--text-3);
		line-height: var(--leading-body);
	}
	.tsp-lead {
		margin: 0 0 var(--space-3);
		font-size: var(--text-md);
		color: var(--text-2);
		line-height: var(--leading-body);
	}
	.tsp-lead strong {
		color: var(--signal-warn);
		font-weight: 600;
	}
	.tsp-readouts {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-6);
		margin-bottom: var(--space-3);
	}
	.tsp-two {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
		gap: var(--space-6);
	}
	.tsp-list,
	.tsp-notes ul {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.tsp-kind-head,
	.tsp-kind {
		display: grid;
		grid-template-columns: 15rem minmax(4rem, 1fr) 4.5rem 5.5rem 5.5rem 7rem;
		align-items: center;
		column-gap: var(--space-3);
	}
	.tsp-kind-head,
	.tsp-build-head {
		padding: 0 var(--space-2) var(--space-1);
		font-size: var(--text-xs);
		color: var(--text-3);
		border-bottom: var(--hairline) solid var(--border-subtle);
	}
	.tsp-kind {
		width: 100%;
		padding: 6px var(--space-2);
		font: inherit;
		font-size: var(--text-sm);
		color: var(--text-2);
		text-align: left;
		background: transparent;
		border: 0;
		border-bottom: var(--hairline) solid var(--border-subtle);
	}
	.tsp-kind--pick {
		cursor: pointer;
	}
	.tsp-kind--pick:hover {
		background: var(--surface-2);
	}
	.tsp-kind--pick:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}
	.tsp-kind--pick[aria-pressed='true'] {
		background: var(--funk-cyan-dim);
	}
	.tsp-label {
		color: var(--text-1);
		overflow-wrap: anywhere;
	}
	.tsp-label--on {
		color: var(--signal-active);
	}
	.tsp-num {
		text-align: right;
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	.tsp-strong {
		color: var(--text-1);
	}
	.tsp-stats {
		display: contents;
	}
	.tsp-inline-label {
		position: absolute;
		width: 1px;
		height: 1px;
		margin: -1px;
		clip-path: inset(50%);
		white-space: nowrap;
	}
	.tsp-build-head,
	.tsp-build {
		display: grid;
		grid-template-columns: 8.5rem minmax(4rem, 1fr) 6.5rem 4rem;
		align-items: center;
		column-gap: var(--space-3);
	}
	.tsp-build {
		padding: 6px var(--space-2);
		font-size: var(--text-sm);
		color: var(--text-2);
		border-bottom: var(--hairline) solid var(--border-subtle);
	}
	.tsp-commands-top {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
	}
	.tsp-show-all--off {
		visibility: hidden;
	}
	.tsp-more {
		margin-top: var(--space-2);
	}
	.tsp-table {
		width: 100%;
		border-collapse: collapse;
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.tsp-table th {
		padding: 0 var(--space-2) var(--space-1);
		font-weight: 400;
		font-size: var(--text-xs);
		color: var(--text-3);
		text-align: left;
		border-bottom: var(--hairline) solid var(--border-subtle);
	}
	.tsp-table th.tsp-num {
		text-align: right;
	}
	.tsp-table td {
		padding: 6px var(--space-2);
		border-bottom: var(--hairline) solid var(--border-subtle);
		vertical-align: top;
	}
	.tsp-empty {
		font-size: var(--text-sm);
		color: var(--text-3);
	}
	.tsp-cmd code {
		font-family: var(--font-mono);
		color: var(--text-1);
		overflow-wrap: anywhere;
		white-space: pre-wrap;
	}
	.tsp-notes li {
		font-size: var(--text-sm);
		color: var(--text-3);
		line-height: var(--leading-body);
		max-width: 80ch;
		padding-left: var(--space-3);
		border-left: var(--hairline) solid var(--border-subtle);
		margin-bottom: var(--space-1);
	}
	.tsp-skeleton {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.tsp-skel-block {
		height: 12px;
		border-radius: 2px;
		background: linear-gradient(
			90deg,
			var(--surface-2) 0%,
			var(--surface-shine) 50%,
			var(--surface-2) 100%
		);
		background-size: 200% 100%;
		margin: 4px 0;
	}

	@container time-spent (max-width: 899.98px) {
		.tsp-two {
			grid-template-columns: minmax(0, 1fr);
		}
	}
	@container time-spent (max-width: 639.98px) {
		.tsp-kind-head,
		.tsp-build-head {
			display: none;
		}
		.tsp-kind {
			grid-template-columns: minmax(0, 1fr) auto;
			row-gap: var(--space-1);
		}
		.tsp-kind > :global([data-slot='share-bar']) {
			grid-column: 1 / -1;
			grid-row: 2;
		}
		.tsp-kind .tsp-stats {
			grid-column: 1 / -1;
			grid-row: 3;
			display: flex;
			flex-wrap: wrap;
			gap: 0 var(--space-3);
			font-size: var(--text-xs);
			color: var(--text-3);
		}
		.tsp-inline-label {
			position: static;
			width: auto;
			height: auto;
			margin: 0;
			clip-path: none;
		}
		.tsp-build {
			grid-template-columns: minmax(0, 1fr) auto auto;
		}
		.tsp-build > :global([data-slot='share-bar']) {
			grid-column: 1 / -1;
			grid-row: 2;
		}
		.tsp-table thead {
			display: none;
		}
		.tsp-table,
		.tsp-table tbody {
			display: block;
		}
		.tsp-table tr {
			display: flex;
			flex-wrap: wrap;
			gap: var(--space-1) var(--space-3);
			padding: 6px var(--space-2);
			border-bottom: var(--hairline) solid var(--border-subtle);
		}
		.tsp-table td {
			padding: 0;
			border: 0;
			font-size: var(--text-xs);
			color: var(--text-3);
			text-align: left;
		}
		.tsp-table td.tsp-cmd {
			flex-basis: 100%;
			font-size: var(--text-sm);
		}
		.tsp-table td[data-label]::before {
			content: attr(data-label) ' ';
		}
	}
</style>
