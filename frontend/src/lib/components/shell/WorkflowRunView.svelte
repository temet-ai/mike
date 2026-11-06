<script module lang="ts">
	export type WorkflowAgentStatus = 'completed' | 'failed' | 'running' | 'pending';
	export type WorkflowPhaseStatus = 'completed' | 'active' | 'pending';

	export interface WorkflowAgentModel {
		label: string;
		model: string;
		status: WorkflowAgentStatus;
		tokens?: number | null;
		toolCalls?: number | null;
		durationMs?: number | null;
		failureReason?: string | null;
		requestIds: number[];
		prompt?: { lines: number; text: string } | null;
		activity?: string[] | null;
		outcome?: string | null;
	}

	export interface WorkflowPhaseModel {
		name: string;
		status: WorkflowPhaseStatus;
		agents: WorkflowAgentModel[];
	}

	export interface WorkflowRunModel {
		name: string;
		description: string;
		phases: WorkflowPhaseModel[];
		elapsedMs?: number | null;
	}
</script>

<script lang="ts">
	import { Check, X, Circle, CircleDot, ChevronRight } from '@lucide/svelte';

	interface Props {
		run: WorkflowRunModel;
	}

	let { run }: Props = $props();

	const defaultPhaseIndex = $derived.by(() => {
		const active = run.phases.findIndex((p) => p.status === 'active');
		return active >= 0 ? active : Math.max(0, run.phases.length - 1);
	});
	let selectedPhaseIndex = $state<number | null>(null);
	let selectedAgentLabel = $state<string | null>(null);

	const phaseIndex = $derived(selectedPhaseIndex ?? defaultPhaseIndex);
	const selectedPhase = $derived(run.phases[phaseIndex] ?? null);
	const selectedAgent = $derived(
		selectedPhase && selectedAgentLabel
			? (selectedPhase.agents.find((a) => a.label === selectedAgentLabel) ?? null)
			: null
	);

	function pickPhase(i: number): void {
		selectedPhaseIndex = i;
		selectedAgentLabel = null;
	}
	function pickAgent(label: string): void {
		selectedAgentLabel = label;
	}
	function backToAgents(): void {
		selectedAgentLabel = null;
	}

	let promptExpanded = $state(false);
	$effect(() => {
		void selectedAgentLabel;
		promptExpanded = false;
	});

	const allAgents = $derived(run.phases.flatMap((p) => p.agents));
	const agentsDone = $derived(allAgents.filter((a) => a.status === 'completed').length);
	const agentsTotal = $derived(allAgents.length);

	function phaseDone(p: WorkflowPhaseModel): number {
		return p.agents.filter((a) => a.status === 'completed').length;
	}

	function fmtTokens(n: number | null | undefined): string | null {
		if (n == null) return null;
		if (n >= 1000) return `${(n / 1000).toFixed(1)}k tok`;
		return `${n} tok`;
	}
	function fmtDuration(ms: number | null | undefined): string | null {
		if (ms == null) return null;
		const s = Math.round(ms / 1000);
		if (s < 60) return `${s}s`;
		const m = Math.floor(s / 60);
		const rem = s % 60;
		if (m < 60) return `${m}m ${rem}s`;
		const h = Math.floor(m / 60);
		return `${h}h ${m % 60}m`;
	}
	function agentMeta(a: WorkflowAgentModel): string {
		return [
			fmtTokens(a.tokens),
			a.toolCalls != null ? `${a.toolCalls} tools` : null,
			fmtDuration(a.durationMs)
		]
			.filter(Boolean)
			.join(' · ');
	}
</script>

<section class="wfv" aria-label={`Workflow run ${run.name}`}>
	<header class="wfv-head">
		<span class="wfv-name">{run.name}</span>
		<span class="wfv-summary">
			{agentsDone}/{agentsTotal} agents{#if run.elapsedMs != null}
				· {fmtDuration(run.elapsedMs)}{/if}
		</span>
	</header>
	<p class="wfv-desc">{run.description}</p>

	<div class="wfv-body">
		<div class="wfv-box wfv-phases" role="group" aria-label="Phases">
			<span class="wfv-box__title">Phases</span>
			<div class="wfv-box__inner">
				{#each run.phases as p, i (p.name)}
					{@const done = phaseDone(p)}
					{@const total = p.agents.length}
					{@const on = i === phaseIndex}
					<button
						type="button"
						class="wfv-phase"
						class:wfv-phase--on={on}
						class:wfv-phase--pending={p.status === 'pending'}
						aria-current={on ? 'true' : undefined}
						onclick={() => pickPhase(i)}
					>
						<span class="wfv-phase__marker" aria-hidden="true">{on ? '›' : ' '}</span>
						{#if p.status === 'completed'}
							<Check size={12} class="wfv-glyph wfv-glyph--ok" aria-hidden="true" />
						{:else}
							<span class="wfv-phase__idx" aria-hidden="true">{i + 1}</span>
						{/if}
						<span class="wfv-phase__name">{p.name}</span>
						{#if total > 0}
							<span class="wfv-phase__count">{done}/{total}</span>
						{/if}
					</button>
				{/each}
			</div>
		</div>

		{#if selectedAgent}
			<div class="wfv-box wfv-detail" role="group" aria-label={selectedAgent.label}>
				<span class="wfv-box__title">
					<button type="button" class="wfv-back" onclick={backToAgents} aria-label="Back to agents">
						<ChevronRight size={11} class="wfv-back__chev" aria-hidden="true" />
					</button>
					{selectedAgent.label}
				</span>
				<div class="wfv-box__inner wfv-detail__inner">
					<div class="wfv-detail__status">
						{#if selectedAgent.status === 'completed'}
							<Check size={12} class="wfv-glyph wfv-glyph--ok" aria-hidden="true" /><span
								class="wfv-st wfv-st--ok">Completed</span
							>
						{:else if selectedAgent.status === 'failed'}
							<X size={12} class="wfv-glyph wfv-glyph--err" aria-hidden="true" /><span
								class="wfv-st wfv-st--err">Failed</span
							>
						{:else if selectedAgent.status === 'running'}
							<CircleDot size={12} class="wfv-glyph wfv-glyph--run" aria-hidden="true" /><span
								class="wfv-st wfv-st--run">Running</span
							>
						{:else}
							<Circle size={12} class="wfv-glyph wfv-glyph--pending" aria-hidden="true" /><span
								class="wfv-st wfv-st--pending">Pending</span
							>
						{/if}
						<span class="wfv-dot">·</span>
						<span class="wfv-model">{selectedAgent.model}</span>
					</div>
					{#if agentMeta(selectedAgent)}
						<div class="wfv-detail__meta">{agentMeta(selectedAgent)}</div>
					{/if}
					{#if selectedAgent.failureReason}
						<div class="wfv-detail__failure">{selectedAgent.failureReason}</div>
					{/if}

					{#if selectedAgent.prompt}
						<div class="wfv-detail__block">
							<div class="wfv-detail__label">
								Prompt · {selectedAgent.prompt.lines} lines
								<button
									type="button"
									class="wfv-expand"
									onclick={() => (promptExpanded = !promptExpanded)}
								>
									{promptExpanded ? 'collapse' : 'expand'}
								</button>
							</div>
							{#if promptExpanded}
								<pre class="wfv-pre">{selectedAgent.prompt.text}</pre>
							{:else}
								<div class="wfv-detail__promptline">{selectedAgent.prompt.text.split('\n')[0]}</div>
								{#if selectedAgent.prompt.lines > 1}
									<div class="wfv-detail__more">… {selectedAgent.prompt.lines - 1} more lines</div>
								{/if}
							{/if}
						</div>
					{/if}

					{#if selectedAgent.activity && selectedAgent.activity.length > 0}
						<div class="wfv-detail__block">
							<div class="wfv-detail__label">
								Activity · last {selectedAgent.activity.length}{#if selectedAgent.toolCalls != null}
									of {selectedAgent.toolCalls} tool calls{/if}
							</div>
							{#each selectedAgent.activity as a (a)}
								<div class="wfv-detail__activity">{a}</div>
							{/each}
						</div>
					{/if}

					{#if selectedAgent.outcome}
						<div class="wfv-detail__block">
							<div class="wfv-detail__label">Outcome</div>
							<pre class="wfv-pre wfv-pre--outcome">{selectedAgent.outcome}</pre>
						</div>
					{/if}
				</div>
			</div>
		{:else if selectedPhase}
			<div class="wfv-box wfv-agents" role="group" aria-label={`${selectedPhase.name} agents`}>
				<span class="wfv-box__title"
					>{selectedPhase.name} · {selectedPhase.agents.length} agents</span
				>
				<div class="wfv-box__inner">
					{#if selectedPhase.agents.length === 0}
						<p class="wfv-empty">No agents dispatched in this phase yet.</p>
					{:else}
						{#each selectedPhase.agents as a (a.label)}
							<button type="button" class="wfv-agent" onclick={() => pickAgent(a.label)}>
								{#if a.status === 'completed'}
									<Check size={12} class="wfv-glyph wfv-glyph--ok" aria-hidden="true" />
								{:else if a.status === 'failed'}
									<X size={12} class="wfv-glyph wfv-glyph--err" aria-hidden="true" />
								{:else if a.status === 'running'}
									<CircleDot size={12} class="wfv-glyph wfv-glyph--run" aria-hidden="true" />
								{:else}
									<Circle size={12} class="wfv-glyph wfv-glyph--pending" aria-hidden="true" />
								{/if}
								<span class="wfv-agent__label">{a.label}</span>
								<span class="wfv-agent__model">{a.model}</span>
								<span class="wfv-agent__meta">
									{agentMeta(a)}{#if a.failureReason}
										· <span class="wfv-agent__fail">failed: {a.failureReason}</span>{/if}
								</span>
							</button>
						{/each}
					{/if}
				</div>
			</div>
		{/if}
	</div>
</section>

<style>
	.wfv {
		font-family: var(--font-mono);
		font-size: 12px;
		color: var(--text-2);
		background: var(--surface-bg);
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		padding: 12px 14px 14px;
	}

	.wfv-head {
		display: flex;
		align-items: baseline;
		gap: 12px;
	}
	.wfv-name {
		font-weight: 700;
		color: var(--interactive-active-fg);
		letter-spacing: 0.01em;
	}
	.wfv-summary {
		margin-left: auto;
		color: var(--text-faint);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	.wfv-desc {
		margin: 3px 0 12px;
		color: var(--text-3);
		line-height: 1.5;
	}

	.wfv-body {
		display: grid;
		grid-template-columns: minmax(150px, 200px) minmax(0, 1fr);
		gap: 12px;
		align-items: start;
	}
	@media (max-width: 640px) {
		.wfv-body {
			grid-template-columns: 1fr;
		}
	}

	.wfv-box {
		position: relative;
		border: 1px solid var(--border-default);
		border-radius: 2px;
		padding: 10px 4px 6px;
	}
	.wfv-box__title {
		position: absolute;
		top: -0.62em;
		left: 10px;
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 0 6px;
		background: var(--surface-bg);
		font-size: 10.5px;
		font-weight: 600;
		letter-spacing: 0.03em;
		color: var(--text-3);
	}
	.wfv-box__inner {
		display: flex;
		flex-direction: column;
		gap: 1px;
	}

	.wfv :global(.wfv-glyph) {
		flex: 0 0 auto;
	}
	.wfv :global(.wfv-glyph--ok) {
		color: var(--status-success-fg);
	}
	.wfv :global(.wfv-glyph--err) {
		color: var(--status-danger-fg);
	}
	.wfv :global(.wfv-glyph--run) {
		color: var(--interactive-active-fg);
	}
	.wfv :global(.wfv-glyph--pending) {
		color: var(--text-faint);
	}

	.wfv-phase {
		display: flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		padding: 3px 6px;
		border: none;
		background: transparent;
		border-radius: 2px;
		cursor: pointer;
		font: inherit;
		color: var(--text-2);
		text-align: left;
	}
	.wfv-phase:hover {
		background: var(--surface-2);
	}
	.wfv-phase:focus-visible {
		outline: none;
		box-shadow: inset 0 0 0 2px var(--interactive-focus-ring);
	}
	.wfv-phase--on {
		color: var(--interactive-active-fg);
	}
	.wfv-phase--pending {
		color: var(--text-faint);
	}
	.wfv-phase__marker {
		width: 8px;
		color: var(--interactive-active-fg);
		font-weight: 700;
	}
	.wfv-phase__idx {
		width: 12px;
		text-align: center;
		color: var(--text-faint);
		font-variant-numeric: tabular-nums;
	}
	.wfv-phase__name {
		flex: 1;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.wfv-phase__count {
		color: var(--text-faint);
		font-variant-numeric: tabular-nums;
	}

	.wfv-agent {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 4px 6px;
		border: none;
		background: transparent;
		border-radius: 2px;
		cursor: pointer;
		font: inherit;
		color: var(--text-2);
		text-align: left;
	}
	.wfv-agent:hover {
		background: var(--surface-2);
	}
	.wfv-agent:focus-visible {
		outline: none;
		box-shadow: inset 0 0 0 2px var(--interactive-focus-ring);
	}
	.wfv-agent__label {
		color: var(--text-strong);
		white-space: nowrap;
		flex: 0 0 auto;
	}
	.wfv-agent__model {
		color: var(--text-3);
		white-space: nowrap;
		flex: 0 0 auto;
	}
	.wfv-agent__meta {
		margin-left: auto;
		color: var(--text-faint);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		min-width: 0;
	}
	.wfv-agent__fail {
		color: var(--status-danger-fg);
	}

	.wfv-back {
		display: inline-flex;
		border: none;
		background: transparent;
		cursor: pointer;
		padding: 0;
		color: var(--text-3);
	}
	.wfv-back:hover {
		color: var(--text-strong);
	}
	.wfv-back :global(.wfv-back__chev) {
		transform: rotate(180deg);
	}
	.wfv-detail__inner {
		gap: 8px;
		padding: 0 6px;
	}
	.wfv-detail__status {
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.wfv-st--ok {
		color: var(--status-success-fg);
	}
	.wfv-st--err {
		color: var(--status-danger-fg);
	}
	.wfv-st--run {
		color: var(--interactive-active-fg);
	}
	.wfv-st--pending {
		color: var(--text-faint);
	}
	.wfv-dot,
	.wfv-model {
		color: var(--text-3);
	}
	.wfv-detail__meta {
		color: var(--text-faint);
		font-variant-numeric: tabular-nums;
	}
	.wfv-detail__failure {
		color: var(--status-danger-fg);
		background: var(--status-danger-bg);
		border: 1px solid var(--status-danger-border);
		border-radius: 2px;
		padding: 4px 8px;
		line-height: 1.5;
	}
	.wfv-detail__block {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.wfv-detail__label {
		display: flex;
		align-items: center;
		gap: 8px;
		color: var(--text-3);
		font-size: 10.5px;
		font-weight: 600;
	}
	.wfv-expand {
		border: none;
		background: transparent;
		cursor: pointer;
		font: inherit;
		font-size: 10px;
		color: var(--interactive-active-fg);
		text-decoration: underline;
		padding: 0;
	}
	.wfv-detail__promptline,
	.wfv-detail__activity {
		color: var(--text-2);
		padding-left: 6px;
	}
	.wfv-detail__more {
		color: var(--text-faint);
		padding-left: 6px;
	}
	.wfv-detail__activity {
		color: var(--text-3);
	}
	.wfv-pre {
		margin: 2px 0 0;
		padding: 6px 8px;
		background: var(--surface-2);
		border-radius: 2px;
		color: var(--text-2);
		line-height: 1.5;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		max-height: 220px;
		overflow: auto;
	}
	.wfv-pre--outcome {
		max-height: 320px;
	}
	.wfv-empty {
		margin: 0;
		padding: 4px 6px;
		color: var(--text-faint);
	}
</style>
