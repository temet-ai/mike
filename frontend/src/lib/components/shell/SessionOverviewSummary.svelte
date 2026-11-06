<script lang="ts">
	import { CircleDashed } from '@lucide/svelte';
	import GoldLabelControl from '$lib/components/eval/GoldLabelControl.svelte';
	import SessionEndChip from './SessionEndChip.svelte';

	export interface SessionQualityDto {
		outcome: 'success' | 'partial' | 'failure' | 'unknown';
		quality_score: number | null;
		rationale: string | null;
		judge_model: string | null;
		rubric_version: number | null;
		judged_at: string | null;
		bundle_id?: number | null;
		verdict_id?: number;
		end_turn_id?: number | null;
		end_label?: string | null;
		end_source?: string | null;
		end_rule?: string | null;
		end_confidence?: string | null;
		end_evidence?: string | null;
		failure?: string | null;
		failure_tags?: string[];
		severity?: string | null;
	}

	interface Props {
		quality: SessionQualityDto | null;
		sessionId?: string;
	}

	let { quality, sessionId = '' }: Props = $props();

	function outcomeAccent(outcome: SessionQualityDto['outcome']): {
		fg: string;
		bg: string;
		border: string;
		label: string;
	} {
		switch (outcome) {
			case 'success':
				return {
					fg: 'var(--status-success-fg)',
					bg: 'var(--status-success-bg)',
					border: 'var(--status-success-border)',
					label: 'SUCCESS'
				};
			case 'partial':
				return {
					fg: 'var(--status-warning-fg)',
					bg: 'var(--status-warning-bg)',
					border: 'var(--status-warning-border)',
					label: 'PARTIAL'
				};
			case 'failure':
				return {
					fg: 'var(--status-danger-fg)',
					bg: 'var(--status-danger-bg)',
					border: 'var(--status-danger-border)',
					label: 'FAILURE'
				};
			case 'unknown':
			default:
				return {
					fg: 'var(--text-3)',
					bg: 'var(--surface-2)',
					border: 'var(--border-subtle)',
					label: 'UNKNOWN'
				};
		}
	}

	function formatJudgedAt(iso: string | null): string {
		if (!iso) return '—';
		try {
			return new Date(iso).toLocaleString(undefined, {
				year: 'numeric',
				month: 'short',
				day: 'numeric',
				hour: '2-digit',
				minute: '2-digit'
			});
		} catch {
			return iso;
		}
	}
</script>

<div class="sos-elev" data-elevation="card">
	<div class="sos-block edge edge-magenta">
		<div class="sos-verdict">
			{#if quality === null}
				<div class="sos-unj">
					<CircleDashed size={16} class="sos-unj__icon" aria-hidden="true" />
					<div class="sos-unj__text">
						<span class="sos-unj__label">Session not yet judged</span>
						<span class="sos-unj__sub">
							The session judge has not yet evaluated this session. Verdicts are produced after the
							session closes.
						</span>
					</div>
				</div>
			{:else}
				{@const accent = outcomeAccent(quality.outcome)}
				<div class="sos-rail" style="background: {accent.fg};" aria-hidden="true"></div>

				<div class="sos-body">
					<div class="sos-top">
						<span
							class="sos-outcome"
							role="status"
							aria-label="Session verdict: {quality.outcome}"
							style="color: {accent.fg}; background: {accent.bg}; border-color: {accent.border};"
						>
							<span class="sos-outcome__dot" style="background: {accent.fg};" aria-hidden="true"
							></span>
							{accent.label}
						</span>

						<span class="sos-score-label">Quality score</span>
						{#if quality.quality_score !== null}
							{@const pct = Math.round(quality.quality_score * 100)}
							<div
								class="sos-meter"
								role="meter"
								aria-label="Quality score: {pct}%"
								aria-valuemin={0}
								aria-valuemax={100}
								aria-valuenow={pct}
							>
								<div class="sos-meter__fill" style="width: {pct}%; background: {accent.fg};"></div>
							</div>
							<span class="sos-score-val" style="color: {accent.fg};">{pct}%</span>
						{:else}
							<span class="sos-score-val sos-score-val--absent">—</span>
						{/if}

						<span class="sos-spacer" aria-hidden="true"></span>

						<span class="sos-section-label">JUDGE VERDICT</span>
					</div>

					{#if quality.end_label}
						<SessionEndChip end={quality} {sessionId} />
					{/if}

					{#if quality.rationale}
						<p class="sos-rationale">
							{quality.rationale}
						</p>
					{/if}

					<div class="sos-meta">
						{#if quality.judge_model}
							<span class="sos-meta__chip">{quality.judge_model}</span>
							<span class="sos-meta__sep" aria-hidden="true">·</span>
						{/if}
						{#if quality.bundle_id}
							<span
								class="sos-meta__chip"
								title="The judge configuration that produced this verdict"
								>bundle #{quality.bundle_id}</span
							>
							<span class="sos-meta__sep" aria-hidden="true">·</span>
						{/if}
						{#if quality.rubric_version}
							<span class="sos-meta__chip">rubric v{quality.rubric_version}</span>
							<span class="sos-meta__sep" aria-hidden="true">·</span>
						{/if}
						{#if quality.judged_at}
							<span>Judged {formatJudgedAt(quality.judged_at)}</span>
						{/if}
					</div>
				</div>
			{/if}
		</div>
		{#if sessionId}
			<div class="sos-label">
				<GoldLabelControl
					subject={{ kind: 'session_end', sessionId }}
					axes={['session_end']}
					context="session_summary"
				/>
			</div>
		{/if}
	</div>
</div>

<style>
	.sos-block {
		--edge: 10px;
		position: relative;
		display: flex;
		flex-direction: column;
		padding: 1px;
		box-shadow: inset 0 1px 0 var(--surface-shine);
	}

	.sos-verdict {
		display: flex;
		min-width: 0;
	}

	.sos-label {
		border-top: 1px solid var(--border-subtle);
		padding: var(--space-3) var(--space-4);
	}

	.sos-rail {
		flex: 0 0 4px;
		min-height: 100%;
	}

	.sos-body {
		flex: 1 1 auto;
		padding: 14px 16px;
		display: flex;
		flex-direction: column;
		gap: 10px;
		min-width: 0;
	}

	.sos-top {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
	}

	.sos-outcome {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 2px 8px;
		border-radius: 2px;
		border: 1px solid transparent;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		font-weight: 700;
		letter-spacing: 0.06em;
		flex-shrink: 0;
	}

	.sos-outcome__dot {
		display: inline-block;
		width: 6px;
		height: 6px;
		flex-shrink: 0;
	}

	.sos-score-label {
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-3);
		flex-shrink: 0;
	}

	.sos-meter {
		flex: 0 0 80px;
		height: 6px;
		background: var(--surface-2);
		border-radius: 2px;
		overflow: hidden;
	}

	.sos-meter__fill {
		height: 100%;
		border-radius: 2px;
		transition: none;
	}

	.sos-score-val {
		font-family: var(--font-mono, monospace);
		font-size: 13px;
		font-weight: 600;
		flex-shrink: 0;
	}

	.sos-score-val--absent {
		color: var(--text-3);
	}

	.sos-spacer {
		flex: 1 1 auto;
	}

	.sos-section-label {
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--text-3);
		flex-shrink: 0;
	}

	.sos-rationale {
		margin: 0;
		max-width: 92ch;
		font-family: var(--font-mono, monospace);
		font-size: 13px;
		line-height: 1.6;
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.sos-meta {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 5px;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
	}

	.sos-meta__chip {
		padding: 1px 5px;
		background: var(--surface-2);
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		color: var(--text-2);
		font-size: 10px;
	}

	.sos-meta__sep {
		color: var(--text-faint);
	}

	.sos-unj {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		padding: 14px 16px;
	}

	:global(.sos-unj__icon) {
		flex: 0 0 auto;
		color: var(--text-faint);
		margin-top: 1px;
	}

	.sos-unj__text {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.sos-unj__label {
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		font-weight: 600;
		color: var(--text-2);
	}

	.sos-unj__sub {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
		line-height: 1.5;
	}
</style>
