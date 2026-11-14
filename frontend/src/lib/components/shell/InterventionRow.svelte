<script lang="ts">
	import { resolve } from '$app/paths';
	import { revertIntervention } from '$lib/api/steering';
	import { changedFilesOf, precheckOf } from '$lib/api/steering';
	import type {
		SessionEvaluationRow,
		SessionSteeringRow,
		SteeringVocabulary,
		SteeringVocabularyItem
	} from '$lib/api/steering';
	import { OUTCOME_TIP, PRECHECK_QUESTION, outcomePhrase } from '$lib/steeringTimeline';
	import { formatRelative } from '$lib/time';
	import PrecheckAnswers from './PrecheckAnswers.svelte';
	import FilesChangedCard from './files-changed-card/FilesChangedCard.svelte';
	import {
		interventionStatus,
		VERDICT_LABELS,
		AUTHORITY_LABELS,
		STEERING_MODE_LABELS
	} from '$lib/steering';
	import type { Verdict, Authority, SteeringMode } from '$lib/steering';
	import { ApiError } from '$lib/api';
	import { RotateCcw } from '@lucide/svelte';
	import { Tooltip, TooltipTrigger, TooltipContent } from '$lib/components/ui/tooltip';
	import InjectedSteerCallout from './InjectedSteerCallout.svelte';
	import WakeStateChip from './WakeStateChip.svelte';
	import SteerVerdictControl from './SteerVerdictControl.svelte';

	interface Props {
		row: SessionSteeringRow;
		sessionBasePath: string;
		vocabulary?: SteeringVocabulary | null;
		evaluation?: SessionEvaluationRow | null;
		held?: boolean;
		evaluationsLoaded?: boolean;
	}

	let {
		row,
		sessionBasePath,
		vocabulary = null,
		evaluation = null,
		held = false,
		evaluationsLoaded = true
	}: Props = $props();

	const precheck = $derived(precheckOf(evaluation));
	const precheckApplies = $derived(
		evaluationsLoaded && evaluation !== null && evaluation.question === PRECHECK_QUESTION
	);
	const outcome = $derived(outcomePhrase(evaluation, held));
	const changed = $derived(evaluationsLoaded ? changedFilesOf(evaluation) : []);
	const changedFacts = $derived(
		Object.fromEntries(
			changed.map((c) => [
				c.path,
				{ kind: c.kind, added: c.added, removed: c.removed, committed: c.committed }
			])
		)
	);
	const verdictWord = $derived(
		held
			? 'Held'
			: row.verdict
				? (VERDICT_LABELS[row.verdict as Verdict] ?? row.verdict)
				: 'unknown'
	);

	function vocabDesc(items: SteeringVocabularyItem[], code: string): string | null {
		return items.find((it) => it.code === code)?.description ?? null;
	}

	let expanded = $state(false);
	let reverting = $state(false);
	let revertError = $state<string | null>(null);

	function toggle(): void {
		expanded = !expanded;
	}

	async function handleRevert(): Promise<void> {
		if (reverting) return;
		const reason = prompt('Revert reason (optional):') ?? '';
		if (reason === null) return;
		reverting = true;
		revertError = null;
		try {
			await revertIntervention(row.intervention_id, { revert_reason: reason });
		} catch (err) {
			revertError = err instanceof ApiError ? err.message : String(err);
		} finally {
			reverting = false;
		}
	}

	const status = $derived(
		interventionStatus({
			sent_at: row.sent_at,
			processed_at: row.processed_at,
			expires_at: row.expires_at,
			reverted_at: row.reverted_at
		})
	);

	const isFired = $derived(row.verdict === 'fired');

	const recessiveClass = $derived.by(() => {
		switch (row.verdict) {
			case 'fired':
				return '';
			case 'suppressed_debounce':
			case 'suppressed_arbitration':
				return 'iv-row--suppressed';
			case 'no_drift':
				return 'iv-row--no-drift';
			case 'gate_miss':
			case 'failed_closed':
				return 'iv-row--faint';
			default:
				return '';
		}
	});

	function formatTime(ts: string | null): string {
		if (!ts) return '—';
		try {
			return new Date(ts).toLocaleTimeString(undefined, {
				hour: '2-digit',
				minute: '2-digit',
				second: '2-digit'
			});
		} catch {
			return ts;
		}
	}

	function formatIso(ts: string | null): string {
		if (!ts) return '—';
		try {
			return new Date(ts)
				.toISOString()
				.replace('T', ' ')
				.replace(/\.\d+Z$/, 'Z');
		} catch {
			return ts;
		}
	}

	function authorityStyle(a: Authority | null): string {
		switch (a) {
			case 'command':
				return 'color: var(--status-warning-fg); background: var(--status-warning-bg); border-color: var(--status-warning-border);';
			case 'redirect':
				return 'color: var(--status-steering-fg); background: var(--status-steering-bg); border-color: var(--status-steering-border);';
			default:
				return 'color: var(--text-2); background: var(--surface-2); border-color: var(--border-subtle);';
		}
	}

	function modeStyle(m: SteeringMode | null): string {
		return m === 'drive'
			? 'color: var(--status-steering-fg); background: var(--status-steering-bg); border-color: var(--status-steering-border);'
			: 'color: var(--text-2); background: var(--surface-2); border-color: var(--border-subtle);';
	}

	function verdictStyle(v: string | null): string {
		if (held) {
			return 'color: var(--text-2); background: var(--surface-2); border-color: var(--border-subtle);';
		}
		switch (v) {
			case 'fired':
				return 'color: var(--status-success-fg); background: var(--status-success-bg); border-color: var(--status-success-border);';
			case 'suppressed_debounce':
			case 'suppressed_arbitration':
				return 'color: var(--text-2); background: var(--surface-2); border-color: var(--border-subtle);';
			case 'no_drift':
				return 'color: var(--text-3); background: var(--surface-2); border-color: var(--border-subtle);';
			case 'gate_miss':
				return 'color: var(--text-3); background: transparent; border-color: transparent;';
			case 'failed_closed':
				return 'color: var(--status-danger-fg); background: var(--status-danger-bg); border-color: var(--status-danger-border);';
			default:
				return 'color: var(--text-3); background: var(--surface-2); border-color: var(--border-subtle);';
		}
	}

	const displayName = $derived(row.steerer_display_name ?? row.steerer_name ?? 'unknown');
</script>

<div
	class="iv-row {recessiveClass}"
	class:iv-row--fired={isFired}
	class:iv-row--reverted={status === 'reverted'}
>
	<button
		type="button"
		class="iv-row__header"
		onclick={toggle}
		aria-expanded={expanded}
		aria-label="Toggle intervention detail for {displayName} ({held
			? 'held'
			: (row.verdict ?? 'unknown verdict')})"
	>
		<div class="iv-row__chrome">
			{#if !held && vocabulary && row.verdict && vocabDesc(vocabulary.verdicts, row.verdict)}
				<Tooltip>
					<TooltipTrigger>
						{#snippet child({ props })}
							<span class="iv-chip iv-tip-trigger" style={verdictStyle(row.verdict)} {...props}>
								{verdictWord}
							</span>
						{/snippet}
					</TooltipTrigger>
					<TooltipContent>
						{vocabDesc(vocabulary.verdicts, row.verdict)}
					</TooltipContent>
				</Tooltip>
			{:else}
				<span class="iv-chip" style={verdictStyle(row.verdict)}>
					{verdictWord}
				</span>
			{/if}

			<span
				class="iv-row__steerer"
				style="color: {isFired ? 'var(--status-steering-fg)' : 'var(--text-2)'};"
			>
				{displayName}
			</span>

			{#if status === 'reverted'}
				<span
					class="iv-chip"
					style="color: var(--status-danger-fg); background: var(--status-danger-bg); border-color: var(--status-danger-border);"
				>
					REVERTED
				</span>
			{/if}

			<span class="iv-row__meta">
				{#if precheck !== null && precheckApplies}
					<Tooltip>
						<TooltipTrigger>
							{#snippet child({ props })}
								<span class="iv-row__precheck iv-tip-trigger" {...props}
									>{precheck.verdict.score.toFixed(2)}</span
								>
							{/snippet}
						</TooltipTrigger>
						<TooltipContent>Pre-check score</TooltipContent>
					</Tooltip>
				{/if}
				{#if outcome !== null}
					<Tooltip>
						<TooltipTrigger>
							{#snippet child({ props })}
								<span class="iv-row__outcome iv-tip-trigger" data-tone={outcome.tone} {...props}
									>{outcome.text}</span
								>
							{/snippet}
						</TooltipTrigger>
						<TooltipContent>{OUTCOME_TIP}</TooltipContent>
					</Tooltip>
				{/if}
				{#if row.authority}
					{#if vocabulary && vocabDesc(vocabulary.authorities, row.authority)}
						<Tooltip>
							<TooltipTrigger>
								{#snippet child({ props })}
									<span
										class="iv-chip iv-chip--sm iv-tip-trigger"
										style={authorityStyle(row.authority as Authority)}
										{...props}
									>
										{AUTHORITY_LABELS[row.authority as Authority] ?? row.authority}
									</span>
								{/snippet}
							</TooltipTrigger>
							<TooltipContent>
								{vocabDesc(vocabulary.authorities, row.authority)}
							</TooltipContent>
						</Tooltip>
					{:else}
						<span class="iv-chip iv-chip--sm" style={authorityStyle(row.authority as Authority)}>
							{AUTHORITY_LABELS[row.authority as Authority] ?? row.authority}
						</span>
					{/if}
				{/if}
				{#if row.mode}
					{#if vocabulary && vocabDesc(vocabulary.modes, row.mode)}
						<Tooltip>
							<TooltipTrigger>
								{#snippet child({ props })}
									<span
										class="iv-chip iv-chip--sm iv-tip-trigger"
										style={modeStyle(row.mode as SteeringMode)}
										{...props}
									>
										{STEERING_MODE_LABELS[row.mode as SteeringMode] ?? row.mode}
									</span>
								{/snippet}
							</TooltipTrigger>
							<TooltipContent>
								{vocabDesc(vocabulary.modes, row.mode)}
							</TooltipContent>
						</Tooltip>
					{:else}
						<span class="iv-chip iv-chip--sm" style={modeStyle(row.mode as SteeringMode)}>
							{STEERING_MODE_LABELS[row.mode as SteeringMode] ?? row.mode}
						</span>
					{/if}
				{/if}
				{#if row.confidence !== null}
					{@const conf = row.confidence}
					<Tooltip>
						<TooltipTrigger>
							{#snippet child({ props })}
								<span class="iv-row__confidence iv-tip-trigger" {...props}>{conf.toFixed(2)}</span>
							{/snippet}
						</TooltipTrigger>
						<TooltipContent>
							the steerer's certainty (0–1) that the session drifted; it only fires when ≥ its
							confidence_threshold
						</TooltipContent>
					</Tooltip>
				{/if}
				<span class="iv-row__time">{formatTime(row.created_at)}</span>
			</span>
		</div>

		{#if row.off_thread_topic}
			<p class="iv-row__topic">{row.off_thread_topic}</p>
		{/if}

		{#if row.sent_request_id !== null}
			<p class="iv-row__link-line">
				→ steered request <a
					href={resolve(
						`${sessionBasePath.slice(1)}/requests/${row.sent_request_id}` as `sessions/${string}/requests/${string}`
					)}
					class="iv-row__link"
					onclick={(e) => e.stopPropagation()}>#{row.sent_request_id}</a
				>
				{#if row.trigger_request_id !== null}
					(triggered by <a
						href={resolve(
							`${sessionBasePath.slice(1)}/requests/${row.trigger_request_id}` as `sessions/${string}/requests/${string}`
						)}
						class="iv-row__link"
						onclick={(e) => e.stopPropagation()}>#{row.trigger_request_id}</a
					>)
				{/if}
			</p>
		{/if}
	</button>

	{#if expanded}
		<div class="iv-row__body">
			{#if status === 'fired' && row.sent_at}
				<div class="iv-banner iv-banner--amber">
					Awaiting session return · Sent {formatIso(row.sent_at)} · Expires {formatIso(
						row.expires_at
					)}
				</div>
			{/if}

			{#if status === 'reverted' && row.revert_reason}
				<div class="iv-banner iv-banner--red">
					<span class="iv-banner__label">Revert reason: </span>
					<span>{row.revert_reason}</span>
				</div>
			{/if}

			{#if row.gate_reason}
				<div class="iv-section">
					<p class="iv-section__label">Gate reason</p>
					<p class="iv-prose">{row.gate_reason}</p>
				</div>
			{/if}

			{#if precheckApplies}
				<div class="iv-section">
					<PrecheckAnswers {precheck} applies={true} />
				</div>
			{/if}

			{#if changed.length > 0}
				<div class="iv-section">
					<FilesChangedCard
						embedded
						files={changed.map((c) => c.path)}
						facts={changedFacts}
						onRetry={() => {}}
					/>
				</div>
			{/if}

			{#if evaluation?.outcome_label && evaluation.labelled_at}
				<p class="iv-labelled">
					Labelled {evaluation.label_source ?? 'unknown'}
					{formatRelative(evaluation.labelled_at)}
				</p>
			{/if}

			{#if row.off_thread_topic}
				<div class="iv-section">
					<p class="iv-section__label">Off-thread topic</p>
					<p class="iv-prose">{row.off_thread_topic}</p>
				</div>
			{/if}

			{#if row.anchor_text}
				<div class="iv-section">
					<p class="iv-section__label">Anchor at evaluation time</p>
					<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
					<pre class="iv-pre" tabindex="0">{row.anchor_text}</pre>
				</div>
			{/if}

			{#if row.payload_text}
				<div class="iv-section">
					<p class="iv-section__label">Injected steer — delivered to the model</p>
					<InjectedSteerCallout payloadText={row.payload_text} />
				</div>
			{/if}
			<WakeStateChip fields={row} />
			{#if row.evaluation_uuid}
				<SteerVerdictControl evaluationUuid={row.evaluation_uuid} />
			{/if}

			{#if row.verdict_json}
				<div class="iv-section">
					<p class="iv-section__label">Verdict JSON</p>
					<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
					<pre class="iv-pre" tabindex="0">{JSON.stringify(row.verdict_json, null, 2)}</pre>
				</div>
			{/if}

			{#if row.sent_request_id !== null}
				<div class="iv-section">
					<p class="iv-section__label">Round-trip</p>
					<div class="iv-roundtrip">
						<a
							href={resolve(
								`${sessionBasePath.slice(1)}/requests/${row.sent_request_id}` as `sessions/${string}/requests/${string}`
							)}
							class="iv-roundtrip__link"
						>
							→ Request #{row.sent_request_id} (steer delivered on this turn)
						</a>
					</div>
				</div>
			{/if}

			<div class="iv-timestamps">
				{#if row.created_at}
					<span>Created: {formatIso(row.created_at)}</span>
				{/if}
				{#if row.sent_at}
					<span>Sent: {formatIso(row.sent_at)}</span>
				{/if}
				{#if row.processed_at}
					<span>Processed: {formatIso(row.processed_at)}</span>
				{/if}
				{#if row.expires_at}
					<span>Expires: {formatIso(row.expires_at)}</span>
				{/if}
			</div>

			{#if isFired && status !== 'reverted'}
				<div class="iv-revert-row">
					<button
						type="button"
						class="iv-revert-btn"
						disabled={reverting}
						onclick={handleRevert}
						aria-label="Revert this intervention"
					>
						<RotateCcw size={11} aria-hidden="true" />
						{reverting ? 'Reverting…' : 'Revert intervention'}
					</button>
					{#if revertError}
						<span class="iv-revert-error" role="alert">{revertError}</span>
					{/if}
				</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	.iv-row {
		display: flex;
		flex-direction: column;
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		background: oklch(from var(--surface-card) l c h / 0.6);
	}

	.iv-row--suppressed {
		background: oklch(from var(--surface-card) l c h / 0.4);
	}

	.iv-row--no-drift {
		background: oklch(from var(--surface-card) l c h / 0.28);
	}

	.iv-row--faint {
		background: oklch(from var(--surface-card) l c h / 0.2);
	}

	.iv-row--reverted {
		background: oklch(from var(--surface-card) l c h / 0.5);
	}

	.iv-row__header {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 8px 12px;
		text-align: left;
		background: transparent;
		border: none;
		cursor: pointer;
		width: 100%;
	}

	.iv-row__header:hover {
		background: oklch(from var(--surface-2) l c h / 0.4);
	}

	.iv-row__header:focus-visible {
		outline: 1px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}

	.iv-row__chrome {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px;
		min-width: 0;
	}

	.iv-chip {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 1px 6px;
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		flex-shrink: 0;
	}

	.iv-chip--sm {
		font-size: 10px;
		padding: 0 4px;
	}

	.iv-row__steerer {
		font-family: var(--font-mono, monospace);
		font-size: 13px;
		font-weight: 500;
		flex-shrink: 0;
	}

	.iv-row__meta {
		margin-left: auto;
		display: flex;
		align-items: center;
		gap: 6px;
		flex-shrink: 0;
		flex-wrap: wrap;
		justify-content: flex-end;
	}

	.iv-row__confidence,
	.iv-row__precheck {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
	}

	.iv-row__time {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
	}

	.iv-row__precheck {
		color: var(--text-2);
	}

	.iv-row__outcome {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
	}

	.iv-row__outcome[data-tone='correct'] {
		color: var(--status-success-fg);
	}

	.iv-row__outcome[data-tone='wrong'] {
		color: var(--status-danger-fg);
	}

	.iv-labelled {
		margin: 0;
		font-size: var(--text-xs);
		color: var(--text-3);
	}

	.iv-row__topic {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 13px;
		color: var(--text-2);
		line-height: 1.5;
		white-space: pre-wrap;
		overflow-wrap: break-word;
		word-break: break-word;
		text-align: left;
	}

	.iv-row__link-line {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
		text-align: left;
	}

	.iv-row__link {
		color: var(--text-2);
		text-decoration: underline;
		text-decoration-color: var(--border-default);
		text-underline-offset: 2px;
	}

	.iv-row__link:hover {
		color: var(--text-1);
	}

	.iv-row__body {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 0 12px 12px;
		border-top: 1px solid var(--border-subtle);
	}

	.iv-banner {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 10px;
		border-radius: 2px;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
	}

	.iv-banner--amber {
		border: 1px solid var(--status-warning-border);
		background: var(--status-warning-bg);
		color: var(--status-warning-fg);
	}

	.iv-banner--red {
		border: 1px solid var(--status-danger-border);
		background: var(--status-danger-bg);
		color: var(--text-1);
	}

	.iv-banner__label {
		font-weight: 600;
		color: var(--status-danger-fg);
		flex-shrink: 0;
	}

	.iv-section {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.iv-section__label {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
	}

	.iv-prose {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 13px;
		color: var(--text-1);
		line-height: 1.6;
		white-space: pre-wrap;
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.iv-pre {
		margin: 0;
		max-height: 300px;
		overflow-y: auto;
		padding: 10px;
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		background: var(--surface-2);
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--text-1);
		line-height: 1.5;
		white-space: pre-wrap;
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.iv-roundtrip {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.iv-roundtrip__link {
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--text-2);
		text-decoration: underline;
		text-decoration-color: var(--border-default);
		text-underline-offset: 2px;
	}

	.iv-roundtrip__link:hover {
		color: var(--text-1);
	}

	.iv-timestamps {
		display: flex;
		flex-wrap: wrap;
		gap: 12px;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
	}

	.iv-revert-row {
		display: flex;
		align-items: center;
		gap: 10px;
		padding-top: 4px;
		border-top: 1px solid var(--border-subtle);
		flex-wrap: wrap;
	}

	.iv-revert-btn {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 4px 10px;
		border: 1px solid var(--status-danger-border);
		border-radius: 2px;
		background: transparent;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--status-danger-fg);
		cursor: pointer;
	}

	.iv-revert-btn:hover {
		background: var(--status-danger-bg);
	}

	.iv-revert-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.iv-revert-error {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--status-danger-fg);
	}

	:global(.iv-tip-trigger) {
		cursor: help;
		text-decoration: underline dotted var(--text-3);
		text-underline-offset: 2px;
	}
</style>
