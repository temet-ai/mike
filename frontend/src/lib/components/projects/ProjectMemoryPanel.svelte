<script lang="ts">
	import { getContext } from 'svelte';
	import { resolve } from '$app/paths';
	import { GraduationCap, Wrench, Rocket, Archive } from '@lucide/svelte';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import { getRecallProjectContext, ApiError } from '$lib/api';
	import type { LessonDto, OperatorTurnDto, RecallProjectContextDto } from '$lib/types';
	import { formatRelative, formatClockAbsolute } from '$lib/time';
	import { splitFirstLine } from '$lib/components/shell/timelineTurnView';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import ArchivedLessonCard from '$lib/components/memory/ArchivedLessonCard.svelte';
	import { getArchivedLessons, restoreLesson, type ArchivedLesson } from '$lib/api/memoryDecisions';
	import { createRetryBackoff } from '$lib/retryBackoff';

	interface Props {
		projectPath: string;
	}

	const { projectPath }: Props = $props();

	const collapseStore = getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY);
	const sectionOpenKey = $derived(`project:${projectPath}:memory-open`);
	let sectionExpandedLocal = $state(false);
	const sectionExpanded = $derived(
		collapseStore ? collapseStore.isCollapsed(sectionOpenKey) : sectionExpandedLocal
	);

	let data = $state<RecallProjectContextDto | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let attemptedForPath = $state<string | null>(null);

	const backoff = createRetryBackoff();

	async function load(path: string): Promise<void> {
		backoff.cancel();
		loading = true;
		error = null;
		try {
			const result = await getRecallProjectContext(path);
			if (path !== projectPath) return;
			data = result;
			backoff.succeeded();
		} catch (err) {
			if (path !== projectPath) return;
			data = null;
			error = err instanceof ApiError ? err.message : String(err);
			backoff.failed(() => {
				if (path === projectPath) void load(path);
			});
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		const path = projectPath;
		if (!sectionExpanded) return;
		if (attemptedForPath === path) return;
		attemptedForPath = path;
		backoff.reset();
		void load(path);
	});

	$effect(() => backoff.cancel);

	function retry(): void {
		backoff.reset();
		void load(projectPath);
	}

	let archivedLessons = $state<ArchivedLesson[]>([]);
	let archivedLoading = $state(false);
	let archivedError = $state<string | null>(null);
	let archivedAttemptedForPath = $state<string | null>(null);

	const archivedBackoff = createRetryBackoff();

	async function loadArchived(path: string): Promise<void> {
		archivedBackoff.cancel();
		archivedLoading = true;
		archivedError = null;
		try {
			const result = await getArchivedLessons(path);
			if (path !== projectPath) return;
			archivedLessons = result;
			archivedBackoff.succeeded();
		} catch (err) {
			if (path !== projectPath) return;
			archivedLessons = [];
			archivedError = err instanceof ApiError ? err.message : String(err);
			archivedBackoff.failed(() => {
				if (path === projectPath) void loadArchived(path);
			});
		} finally {
			archivedLoading = false;
		}
	}

	$effect(() => {
		const path = projectPath;
		if (!sectionExpanded) return;
		if (archivedAttemptedForPath === path) return;
		archivedAttemptedForPath = path;
		archivedBackoff.reset();
		void loadArchived(path);
	});

	$effect(() => archivedBackoff.cancel);

	function retryArchived(): void {
		archivedBackoff.reset();
		void loadArchived(projectPath);
	}

	let restoringLessonId = $state<number | null>(null);
	let restoreErrors = $state<Record<number, string>>({});
	let restoredLessonIds = $state<Set<number>>(new Set());
	let restoreAnnouncement = $state('');

	async function handleRestore(lesson: ArchivedLesson): Promise<void> {
		restoringLessonId = lesson.id;
		const nextErrors = { ...restoreErrors };
		delete nextErrors[lesson.id];
		restoreErrors = nextErrors;
		try {
			await restoreLesson(lesson.id);
			// eslint-disable-next-line svelte/prefer-svelte-reactivity -- builds a fresh Set copy and reassigns it wholesale to `restoredLessonIds`; never mutates the reactive value in place.
			restoredLessonIds = new Set(restoredLessonIds).add(lesson.id);
			restoreAnnouncement = `Lesson #${lesson.id} restored.`;
		} catch (err) {
			restoreErrors = {
				...restoreErrors,
				[lesson.id]: err instanceof ApiError ? err.message : String(err)
			};
			restoreAnnouncement = `Couldn't restore lesson #${lesson.id}.`;
		} finally {
			restoringLessonId = null;
		}
	}

	const memoryHref = $derived(`/memory?project=${encodeURIComponent(projectPath)}` as const);

	let expandedTurns = $state<Set<number>>(new Set());
	function toggleTurn(turnId: number): void {
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- `next` is a plain local copy, mutated then reassigned wholesale to `expandedTurns` below; never mutated in place.
		const next = new Set(expandedTurns);
		if (next.has(turnId)) next.delete(turnId);
		else next.add(turnId);
		expandedTurns = next;
	}

	const isEmpty = $derived(
		data !== null &&
			data.lessons.length === 0 &&
			data.recent_corrected.length === 0 &&
			data.recent_shipped.length === 0
	);
</script>

{#snippet turnRow(turn: OperatorTurnDto, accentVar: string)}
	{@const open = expandedTurns.has(turn.turn_id)}
	{@const promptPreview = splitFirstLine(turn.user_prompt)}
	{@const replyText = turn.stop_response ?? turn.final_assistant_text}
	{@const replyPreview = splitFirstLine(replyText)}
	<li>
		<button
			type="button"
			class="pmp-turn"
			style="--pmp-accent: var({accentVar});"
			aria-expanded={open}
			aria-label={`${open ? 'Collapse' : 'Expand'} turn from ${formatRelative(turn.created_at)}`}
			onclick={() => toggleTurn(turn.turn_id)}
			data-slot="project-memory-turn"
		>
			<div class="pmp-turn__head">
				<span
					class="pmp-turn__req font-mono text-micro tabular-nums"
					title={`Requests #${turn.start_request_number}–#${turn.end_request_number}`}
				>
					{turn.start_request_number === turn.end_request_number
						? `#${turn.start_request_number}`
						: `#${turn.start_request_number}–#${turn.end_request_number}`}
				</span>
				<span
					class="pmp-turn__when font-mono text-micro tabular-nums text-[var(--text-faint)]"
					title={formatClockAbsolute(turn.created_at)}
				>
					{formatRelative(turn.created_at)}
				</span>
			</div>
			{#if open}
				<div class="pmp-turn__body">
					<div class="pmp-turn__field">
						<span class="pmp-turn__label">Prompt</span>
						<p class="pmp-turn__text">{turn.user_prompt}</p>
					</div>
					<div class="pmp-turn__field">
						<span class="pmp-turn__label">Reply</span>
						{#if replyText.length > 0}
							<p class="pmp-turn__text">{replyText}</p>
						{:else}
							<p class="pmp-turn__text pmp-turn__text--empty">No reply captured.</p>
						{/if}
					</div>
				</div>
			{:else}
				<p class="pmp-turn__collapsed-line">{promptPreview.first}</p>
				{#if replyPreview.first.length > 0}
					<p class="pmp-turn__collapsed-line pmp-turn__collapsed-line--reply">
						{replyPreview.first}
					</p>
				{/if}
				<span class="pmp-turn__hint">click turn to expand</span>
			{/if}
		</button>
	</li>
{/snippet}

{#snippet lessonRow(lesson: LessonDto)}
	<li class="pmp-lesson" data-slot="project-memory-lesson">
		<div class="pmp-lesson__head">
			<span class="pmp-lesson__kind">{lesson.kind}</span>
			{#if lesson.topic}
				<span class="pmp-lesson__topic">{lesson.topic}</span>
			{/if}
			<span
				class="pmp-lesson__when font-mono text-micro tabular-nums text-[var(--text-faint)]"
				title={formatClockAbsolute(lesson.created_at)}
			>
				{formatRelative(lesson.created_at)}
			</span>
		</div>
		<p class="pmp-lesson__text">{lesson.text}</p>
	</li>
{/snippet}

<div data-slot="project-memory-panel">
	<SectionCard
		title="Project memory"
		collapsible
		bordered={false}
		sectionKey={sectionOpenKey}
		defaultCollapsed
		titleTooltip="The same lessons, corrected turns, and shipped turns bundle a fresh agent recalls for this project (GET /api/recall/project-context)."
	>
		{#snippet headerActions()}
			{#if data}
				<span class="pmp-meta">
					{data.lessons.length} lesson{data.lessons.length === 1 ? '' : 's'} · {data
						.recent_corrected.length} corrected · {data.recent_shipped.length} shipped
				</span>
			{/if}
			<a class="pmp-memory-link" href={resolve(memoryHref)}>View in memory →</a>
		{/snippet}
		{#snippet children()}
			<span class="sr-only" role="status" aria-live="polite">{restoreAnnouncement}</span>
			<div class="pmp-body">
				{#if loading && !data}
					<div class="pmp-skeleton" aria-hidden="true">
						<div class="pmp-skel-row"></div>
						<div class="pmp-skel-row pmp-skel-row--short"></div>
					</div>
				{/if}

				{#if error}
					<LoadErrorBanner
						title="Couldn't load project memory"
						detail={error}
						hint="The recall endpoint may not be reachable, or this project has no recorded memory yet."
						onRetry={retry}
					/>
				{/if}

				{#if data && !error}
					{#if isEmpty}
						<p class="pmp-empty" data-slot="project-memory-empty">
							No recorded lessons or turns for this project yet.
						</p>
					{:else}
						<div class="pmp-section">
							<div class="pmp-section__head">
								<GraduationCap size={13} aria-hidden="true" />
								<h4 class="pmp-section__title">Lessons</h4>
							</div>
							{#if data.lessons.length === 0}
								<p class="pmp-empty">No active lessons recorded for this project.</p>
							{:else}
								<ul class="pmp-lesson-list" role="list">
									{#each data.lessons as lesson (lesson.id)}
										{@render lessonRow(lesson)}
									{/each}
								</ul>
							{/if}
						</div>

						<div class="pmp-section">
							<div class="pmp-section__head">
								<Wrench size={13} aria-hidden="true" />
								<h4 class="pmp-section__title">Recently corrected</h4>
								<span class="pmp-section__sub">last {data.recent_corrected.length}</span>
							</div>
							{#if data.recent_corrected.length === 0}
								<p class="pmp-empty">No corrected turns recorded for this project.</p>
							{:else}
								<ul class="pmp-turn-list" role="list">
									{#each data.recent_corrected as turn (turn.turn_id)}
										{@render turnRow(turn, '--status-danger-fg')}
									{/each}
								</ul>
							{/if}
						</div>

						<div class="pmp-section">
							<div class="pmp-section__head">
								<Rocket size={13} aria-hidden="true" />
								<h4 class="pmp-section__title">Recently shipped</h4>
								<span class="pmp-section__sub">last {data.recent_shipped.length}</span>
							</div>
							{#if data.recent_shipped.length === 0}
								<p class="pmp-empty">No shipped turns recorded for this project.</p>
							{:else}
								<ul class="pmp-turn-list" role="list">
									{#each data.recent_shipped as turn (turn.turn_id)}
										{@render turnRow(turn, '--status-info-fg')}
									{/each}
								</ul>
							{/if}
						</div>
					{/if}
				{/if}

				<div class="pmp-section">
					<div class="pmp-section__head">
						<Archive size={13} aria-hidden="true" />
						<h4 class="pmp-section__title">Archived</h4>
						{#if archivedLessons.length > 0}
							<span class="pmp-section__sub">{archivedLessons.length}</span>
						{/if}
					</div>
					{#if archivedLoading && archivedLessons.length === 0}
						<div class="pmp-skeleton" aria-hidden="true">
							<div class="pmp-skel-row"></div>
						</div>
					{:else if archivedError}
						<LoadErrorBanner
							title="Couldn't load archived lessons"
							detail={archivedError}
							hint="The lessons-archived endpoint may not be reachable yet."
							onRetry={retryArchived}
						/>
					{:else if archivedLessons.length === 0}
						<p class="pmp-empty">No archived lessons for this project.</p>
					{:else}
						<ul class="pmp-archived-list" role="list">
							{#each archivedLessons as lesson (lesson.id)}
								<li>
									<ArchivedLessonCard
										{lesson}
										onRestore={handleRestore}
										restoring={restoringLessonId === lesson.id}
										restoreError={restoreErrors[lesson.id] ?? null}
										restored={restoredLessonIds.has(lesson.id)}
									/>
								</li>
							{/each}
						</ul>
					{/if}
				</div>
			</div>
		{/snippet}
	</SectionCard>
</div>

<style>
	.pmp-meta {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption, 11px);
		color: var(--text-3);
		white-space: nowrap;
	}

	.pmp-memory-link {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption, 11px);
		color: var(--text-3);
		white-space: nowrap;
		text-decoration: none;
	}
	.pmp-memory-link:hover {
		color: var(--interactive-active-fg);
	}

	.pmp-body {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}

	.pmp-skeleton {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.pmp-skel-row {
		height: 14px;
		border-radius: var(--r-tol);
		background: var(--surface-2);
	}
	.pmp-skel-row--short {
		width: 60%;
	}

	.pmp-empty {
		margin: 0;
		font-size: var(--text-sm);
		font-style: italic;
		color: var(--text-2);
	}

	.pmp-section {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.pmp-section__head {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		color: var(--text-3);
	}

	.pmp-section__title {
		margin: 0;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-size: var(--text-xs);
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: var(--track-machine);
		color: var(--text-2);
	}

	.pmp-section__sub {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-xs);
		color: var(--text-2);
	}

	.pmp-lesson-list {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.pmp-lesson {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding: var(--space-2) var(--space-3);
		border: var(--hairline) solid var(--line);
		border-radius: var(--r-tol);
		background: var(--surface-2);
	}

	.pmp-lesson__head {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		flex-wrap: wrap;
	}

	.pmp-lesson__kind {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-xs);
		font-weight: var(--weight-display);
		text-transform: uppercase;
		letter-spacing: var(--track-display);
		color: var(--text-2);
	}

	.pmp-lesson__topic {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-xs);
		color: var(--text-2);
	}

	.pmp-lesson__when {
		margin-left: auto;
	}

	.pmp-lesson__text {
		margin: 0;
		white-space: pre-wrap;
		word-break: break-word;
		font-size: var(--text-sm);
		line-height: var(--leading-body);
		color: var(--text-1);
	}

	.pmp-turn-list {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.pmp-archived-list {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.pmp-turn {
		display: block;
		width: 100%;
		padding: var(--space-2) var(--space-3);
		border: var(--hairline) solid var(--line);
		border-left: 2px solid var(--pmp-accent, var(--line));
		border-radius: var(--r-tol);
		background: var(--surface-2);
		text-align: left;
		font: inherit;
		cursor: pointer;
		transition: background-color var(--dur-1) var(--ease-io);
	}

	.pmp-turn:hover {
		background: var(--surface-3);
	}

	.pmp-turn:focus-visible {
		outline: 2px solid var(--focus);
		outline-offset: -2px;
	}

	.pmp-turn__head {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin-bottom: var(--space-1);
	}

	.pmp-turn__req {
		color: var(--text-2);
	}

	.pmp-turn__when {
		margin-left: auto;
	}

	.pmp-turn__collapsed-line {
		margin: 0;
		font-size: var(--text-sm);
		line-height: var(--leading-body);
		color: var(--text-1);
		white-space: pre-wrap;
		word-break: break-word;
	}

	.pmp-turn__collapsed-line--reply {
		color: var(--text-2);
	}

	.pmp-turn__hint {
		display: block;
		margin-top: var(--space-1);
		font-size: var(--text-xs);
		color: var(--text-2);
	}

	.pmp-turn__body {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.pmp-turn__field {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.pmp-turn__label {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-xs);
		font-weight: var(--weight-display);
		text-transform: uppercase;
		letter-spacing: var(--track-display);
		color: var(--text-2);
	}

	.pmp-turn__text {
		margin: 0;
		white-space: pre-wrap;
		word-break: break-word;
		font-size: var(--text-sm);
		line-height: var(--leading-body);
		color: var(--text-1);
	}

	.pmp-turn__text--empty {
		font-style: italic;
		color: var(--text-2);
	}
</style>
