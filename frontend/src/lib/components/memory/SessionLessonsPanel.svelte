<script lang="ts">
	import { BookOpen } from '@lucide/svelte';
	import { getContext } from 'svelte';
	import { resolve } from '$app/paths';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import { ApiError } from '$lib/api';
	import { getSessionLessons, type SessionLessons } from '$lib/api/sessionLessons';
	import { formatRelative, formatClockAbsolute } from '$lib/time';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import { createRetryBackoff } from '$lib/retryBackoff';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';

	interface Props {
		sessionId: string;
		grouped?: boolean;
	}

	let { sessionId, grouped = false }: Props = $props();

	const collapseStore = getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY);
	const sectionKey = $derived(`session:${sessionId}:lessons-open`);
	let sectionExpandedLocal = $state(false);
	const sectionExpanded = $derived(
		collapseStore ? collapseStore.isExpanded(sectionKey) : sectionExpandedLocal
	);

	const memoryHref = $derived(`/memory?session=${encodeURIComponent(sessionId)}` as const);

	let data = $state<SessionLessons | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let attemptedForSession = $state<string | null>(null);
	let statusAnnouncement = $state('');

	const backoff = createRetryBackoff();

	async function load(id: string): Promise<void> {
		backoff.cancel();
		loading = true;
		error = null;
		try {
			const result = await getSessionLessons(id);
			if (id !== sessionId) return;
			data = result;
			backoff.succeeded();
			statusAnnouncement = `Loaded: ${result.injected_count} lesson${result.injected_count === 1 ? '' : 's'} shown to this session, ${result.recorded_count} recorded from it.`;
		} catch (err) {
			if (id !== sessionId) return;
			error = err instanceof ApiError ? err.message : String(err);
			statusAnnouncement = "Couldn't load this session's lessons.";
			backoff.failed(() => {
				if (id === sessionId) void load(id);
			});
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		const id = sessionId;
		if (!sectionExpanded) return;
		if (attemptedForSession === id) return;
		attemptedForSession = id;
		backoff.reset();
		void load(id);
	});

	$effect(() => backoff.cancel);

	function retry(): void {
		backoff.reset();
		void load(sessionId);
	}
</script>

<SectionCard
	title="Lessons touching this session"
	titleIcon={BookOpen}
	collapsible
	bordered={!grouped}
	{sectionKey}
	defaultCollapsed
	titleTooltip="Lessons this session was shown mid-turn, and lessons recorded with this session as their source."
>
	{#snippet headerActions()}
		{#if data}
			<span class="slp-meta">{data.injected_count} shown · {data.recorded_count} recorded</span>
		{/if}
	{/snippet}
	{#snippet children()}
		<span class="sr-only" role="status" aria-live="polite">{statusAnnouncement}</span>
		<div class="slp-body">
			{#if loading && !data}
				<div class="slp-skeleton" aria-hidden="true">
					<div class="slp-skel-row"></div>
					<div class="slp-skel-row slp-skel-row--short"></div>
				</div>
			{/if}

			{#if error}
				<LoadErrorBanner
					title="Couldn't load this session's lessons"
					detail={error}
					hint="The session-lessons endpoint may not be reachable yet."
					onRetry={retry}
				/>
			{:else if data}
				<div class="slp-subsection">
					<h4 class="slp-subheading">Shown to this session</h4>
					{#if data.injected.length === 0}
						<p class="slp-empty">No lessons were injected into this session.</p>
					{:else}
						<ul class="slp-list" role="list">
							{#each data.injected as lesson (lesson.lesson_id)}
								<li class="slp-row" data-signal="active" data-slot="session-lesson-injected">
									<div class="slp-row__head">
										<span class="slp-chip">lesson #{lesson.lesson_id}</span>
										<span class="slp-chip">{lesson.kind}</span>
										<span class="slp-chip">shown {lesson.injection_count}×</span>
										<span
											class="slp-when font-mono text-micro tabular-nums text-[var(--text-faint)]"
											title={`first shown ${formatClockAbsolute(lesson.first_injected_at)} · last shown ${formatClockAbsolute(lesson.last_injected_at)}`}
										>
											last shown {formatRelative(lesson.last_injected_at)}
										</span>
									</div>
									<p class="slp-text">{lesson.lesson_text}</p>
								</li>
							{/each}
						</ul>
					{/if}
				</div>

				<div class="slp-subsection">
					<h4 class="slp-subheading">Recorded from this session</h4>
					{#if data.recorded.length === 0}
						<p class="slp-empty">
							No lessons have been recorded with this session as their source yet. Session
							provenance on recorded lessons started being tracked recently, so this fills in for
							sessions going forward — not retroactively.
						</p>
					{:else}
						<ul class="slp-list" role="list">
							{#each data.recorded as lesson (lesson.lesson_id)}
								<li class="slp-row" data-slot="session-lesson-recorded">
									<div class="slp-row__head">
										<span class="slp-chip">lesson #{lesson.lesson_id}</span>
										<span class="slp-chip">{lesson.kind}</span>
										<span
											class="slp-when font-mono text-micro tabular-nums text-[var(--text-faint)]"
											title={formatClockAbsolute(lesson.created_at)}
										>
											recorded {formatRelative(lesson.created_at)}
										</span>
									</div>
									<p class="slp-text">{lesson.lesson_text}</p>
								</li>
							{/each}
						</ul>
					{/if}
				</div>

				<a class="slp-memory-link" href={resolve(memoryHref)}
					>View this session's memory on /memory →</a
				>
			{/if}
		</div>
	{/snippet}
</SectionCard>

<style>
	.slp-meta {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption, 11px);
		color: var(--text-3);
		white-space: nowrap;
	}

	.slp-body {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}

	.slp-subsection {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.slp-subheading {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-xs);
		font-weight: var(--weight-display);
		text-transform: uppercase;
		letter-spacing: var(--track-display);
		color: var(--text-2);
	}

	.slp-empty {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
		font-style: italic;
	}

	.slp-list {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.slp-row {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		border: var(--hairline) solid var(--line);
		border-radius: var(--r-tol);
		padding: var(--space-2) var(--space-3);
		background: var(--surface-2);
	}

	.slp-row[data-signal='active'] {
		border-color: var(--cyan-border);
		box-shadow: inset 2px 0 0 0 var(--signal-active);
	}

	.slp-row__head {
		display: flex;
		align-items: center;
		gap: var(--space-1);
		flex-wrap: wrap;
	}

	.slp-chip {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-xs);
		text-transform: uppercase;
		letter-spacing: var(--track-display);
		color: var(--text-2);
		background: var(--surface-3);
		border-radius: var(--r-tol);
		padding: 1px var(--space-2);
	}

	.slp-when {
		margin-left: auto;
	}

	.slp-text {
		margin: 0;
		white-space: pre-wrap;
		word-break: break-word;
		font-size: var(--text-sm);
		line-height: var(--leading-body);
		color: var(--text-1);
	}

	.slp-memory-link {
		align-self: flex-start;
		font-size: var(--text-xs);
		color: var(--cyan);
		text-decoration: none;
	}
	.slp-memory-link:hover {
		text-decoration: underline;
	}
	.slp-memory-link:focus-visible {
		outline: 2px solid var(--focus);
		outline-offset: 2px;
		border-radius: var(--r-tol);
	}

	.slp-skeleton {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.slp-skel-row {
		height: 14px;
		border-radius: var(--r-tol);
		background: var(--surface-2);
	}
	.slp-skel-row--short {
		width: 60%;
	}
</style>
