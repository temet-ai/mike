<script lang="ts">
	import { onMount } from 'svelte';
	import {
		WRITABLE_MODES,
		formatRate,
		getGuard,
		getGuardMetrics,
		setGuardMode,
		setGuardToggle
	} from '$lib/api/guard';
	import type { GuardMetrics, GuardMode, GuardQuestion, GuardSummary } from '$lib/api/guard';
	import {
		demoteQuestion,
		getSteerQuestions,
		promoteQuestion,
		setHoldback
	} from '$lib/api/steerQuestions';
	import type { SteerQuestionStatus, SteerQuestionsResponse } from '$lib/api/steerQuestions';

	const MODES: GuardMode[] = ['off', 'shadow', 'append', 'wake'];

	let summary = $state<GuardSummary | null>(null);
	let metrics = $state<GuardMetrics | null>(null);
	let loadError = $state<string | null>(null);
	let rowError = $state<Record<string, string | null>>({});
	let promotion = $state<SteerQuestionsResponse | null>(null);
	let overrideReason = $state<Record<string, string>>({});
	let holdbackInput = $state<Record<string, string>>({});

	async function load(): Promise<void> {
		try {
			summary = await getGuard();
			metrics = await getGuardMetrics();
			promotion = await getSteerQuestions().catch(() => null);
			loadError = null;
		} catch (e) {
			loadError = e instanceof Error ? e.message : String(e);
		}
	}

	async function changeMode(question: GuardQuestion, mode: GuardMode): Promise<void> {
		try {
			await setGuardMode(question, mode);
			rowError[question] = null;
			await load();
		} catch (e) {
			rowError[question] = e instanceof Error ? e.message : String(e);
		}
	}

	async function toggle(): Promise<void> {
		if (!summary) return;
		try {
			await setGuardToggle(!summary.enabled);
			await load();
		} catch (e) {
			loadError = e instanceof Error ? e.message : String(e);
		}
	}

	function statusFor(question: string): SteerQuestionStatus | null {
		return promotion?.questions.find((q) => q.question === question) ?? null;
	}

	async function act(question: string, run: () => Promise<unknown>): Promise<void> {
		try {
			await run();
			rowError[question] = null;
			await load();
		} catch (e) {
			rowError[question] = e instanceof Error ? e.message : String(e);
		}
	}

	function promoteTo(question: string, target: string): Promise<void> {
		const id = promotion?.steerer_id ?? 0;
		return act(question, () => promoteQuestion(id, question, target, overrideReason[question]));
	}

	function demoteToShadow(question: string): Promise<void> {
		const id = promotion?.steerer_id ?? 0;
		return act(question, () =>
			demoteQuestion(id, question, 'shadow', 'operator demotion from the panel')
		);
	}

	function saveHoldback(question: string): Promise<void> {
		const id = promotion?.steerer_id ?? 0;
		const raw = (holdbackInput[question] ?? '').trim();
		return act(question, () => setHoldback(id, question, raw === '' ? null : Number(raw)));
	}

	function metricsFor(question: string) {
		return (metrics?.metrics ?? []).filter((m) => m.question === question);
	}

	onMount(() => {
		void load();
	});
</script>

<section class="flex flex-col gap-3" data-testid="guard-questions">
	<div class="flex items-center gap-3">
		<h2 class="text-sm font-semibold text-[var(--text-strong)]">Turn guard</h2>
		{#if summary}
			<button
				type="button"
				class="text-micro font-semibold"
				aria-pressed={summary.enabled}
				data-guard-toggle
				onclick={() => void toggle()}>{summary.enabled ? 'On' : 'Off'}</button
			>
			<span class="text-micro text-[var(--text-3)]">
				{summary.rubric_version} · {summary.gate_version} · labeller {summary.labeller.state}
			</span>
		{/if}
	</div>
	{#if loadError}
		<p class="text-micro text-[var(--text-3)]" role="alert">
			Could not load the guard: {loadError}
		</p>
	{:else if summary}
		{#if metrics}
			<p
				class="text-micro {metrics.audit.audit_set_fires > 0
					? 'text-[var(--red)]'
					: 'text-[var(--text-3)]'}"
				data-guard-audit
			>
				{metrics.audit.audit_set_fires} would-fires on {metrics.audit.audit_set_sessions} seat and template
				sessions since {metrics.since.split('T')[0]}
			</p>
		{/if}
		{#each summary.questions as q (q.question)}
			<article
				class="flex flex-col gap-1 border border-[var(--border-subtle)] p-3"
				data-guard-question={q.question}
			>
				<div class="flex flex-wrap items-center gap-2">
					<span class="text-sm font-semibold text-[var(--text-strong)]"
						>{q.question.replaceAll('_', ' ')}</span
					>
					<select
						class="border border-[var(--border-subtle)] bg-[var(--surface-bg)] px-2 py-1 text-micro"
						aria-label={`Mode for ${q.question}`}
						value={q.mode}
						onchange={(e) =>
							void changeMode(
								q.question,
								(e.currentTarget as HTMLSelectElement).value as GuardMode
							)}
					>
						{#each MODES as m (m)}
							<option value={m} disabled={!WRITABLE_MODES.includes(m)}
								>{WRITABLE_MODES.includes(m) ? m : `${m} (JC17)`}</option
							>
						{/each}
					</select>
					<span class="text-micro text-[var(--text-3)]">{q.dependency_state}</span>
				</div>
				<p class="text-micro text-[var(--text-2)]">
					last 24 h: {q.counts_24h.rows} evaluations, {q.counts_24h.would_fire} would-fire · threshold
					{q.confidence_threshold}
					· bar {q.bar}
				</p>
				{#each metricsFor(q.question) as m (m.trigger_point)}
					<p
						class="text-micro text-[var(--text-2)]"
						data-guard-metric={`${m.question}:${m.trigger_point}`}
					>
						{m.trigger_point}: precision {formatRate(m.precision)} · recall
						{m.recall === null ? 'n/a' : `${Math.round(m.recall * 100)}%`} · labelled {m.funnel
							.labelled} of {m.funnel.rows}
					</p>
				{/each}
				{#if statusFor(q.question)}
					{@const st = statusFor(q.question)!}
					<div class="flex flex-wrap items-center gap-2" data-guard-promotion={q.question}>
						{#each st.bar.targets as target (target)}
							{@const g = st.gate[target]}
							<span class="text-micro text-[var(--text-2)]" data-gate-target={target}>
								{target}: {g?.passes
									? 'gate passes'
									: `gate fails (${(g?.reasons ?? []).join('; ')})`}
							</span>
							<button
								type="button"
								class="text-micro font-semibold"
								disabled={!g?.passes && !(overrideReason[q.question] ?? '').trim()}
								onclick={() => void promoteTo(q.question, target)}>Promote to {target}</button
							>
						{/each}
						<input
							class="border border-[var(--border-subtle)] bg-[var(--surface-bg)] px-2 py-1 text-micro"
							placeholder="Override reason"
							aria-label={`Override reason for ${q.question}`}
							bind:value={overrideReason[q.question]}
						/>
						{#if st.policy.mode === 'append' || st.policy.mode === 'wake'}
							<button
								type="button"
								class="text-micro"
								onclick={() => void demoteToShadow(q.question)}>Demote to shadow</button
							>
						{/if}
						<input
							class="w-16 border border-[var(--border-subtle)] bg-[var(--surface-bg)] px-2 py-1 text-micro"
							placeholder={String(st.effective_holdback_pct)}
							aria-label={`Holdback percent for ${q.question}`}
							bind:value={holdbackInput[q.question]}
						/>
						<button type="button" class="text-micro" onclick={() => void saveHoldback(q.question)}
							>Set holdback</button
						>
						<span class="text-micro text-[var(--text-3)]">
							arms {st.live.arms.treated} treated / {st.live.arms.holdback} held back · effect {st
								.effect.status}
							· {st.promotions.length} promotion changes
						</span>
					</div>
				{/if}
				{#if rowError[q.question]}
					<p class="text-micro text-[var(--text-3)]" role="alert">{rowError[q.question]}</p>
				{/if}
			</article>
		{/each}
		<p class="text-micro text-[var(--text-3)]">
			A promotion applies to fires within a minute; a demotion also cancels pending wakes at the
			next tick after that.
		</p>
		<p class="text-micro text-[var(--text-3)]">
			Calls per day {summary.settings.max_calls_per_day} · labelling calls per day {summary.settings
				.max_label_calls_per_day}
			· recall sample {summary.settings.recall_sample_pct}%
		</p>
	{/if}
</section>
