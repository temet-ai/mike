<script lang="ts">
	import { ArrowUpRight } from '@lucide/svelte';
	import type { AgentEntry, AgentEvent, DispatchOutcome } from '$lib/agentDispatches';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';

	export type { AgentEntry, AgentEvent, DispatchOutcome };

	export type VerdictCode =
		| 'fired'
		| 'suppressed_debounce'
		| 'suppressed_arbitration'
		| 'no_drift'
		| 'gate_miss'
		| 'failed_closed';

	export type AuthorityCode = 'supply_context' | 'redirect' | 'command';
	export type ModeCode = 'augment' | 'drive';

	export interface SteererEvent {
		createdAt: string;
		verdict: VerdictCode;
		confidence: number | null;
		gateReason: string | null;
		payloadText: string | null;
		triggerRequestId: number | null;
		sentRequestId: number | null;
	}

	export interface SteererEntry {
		steererDisplayName: string | null;
		steererName: string | null;
		description: string | null;
		authority: AuthorityCode | null;
		mode: ModeCode | null;
		enrolled: boolean;
		events: SteererEvent[];
	}

	const VERDICT_LABELS: Record<VerdictCode, string> = {
		fired: 'Fired',
		suppressed_debounce: 'Held back (debounce)',
		suppressed_arbitration: 'Held back (arbitration)',
		no_drift: 'No drift detected',
		gate_miss: 'Gate not met',
		failed_closed: 'Failed safe'
	};

	const VERDICT_TOOLTIPS: Record<VerdictCode, string> = {
		fired: 'The steerer judged drift above threshold and injected a nudge on the next turn.',
		suppressed_debounce:
			'Held back (debounce): the steerer judged drift and would have fired, but was suppressed ' +
			'because it already fired within the last debounce_turns turns ' +
			'(a re-fire cooldown so it does not nag every turn).',
		suppressed_arbitration:
			'Held back (arbitration): another steerer with higher priority fired first this turn.',
		no_drift: 'No drift detected: the steerer evaluated but confidence was below threshold.',
		gate_miss:
			'Gate not met: the gate condition was not triggered — evaluator did not run this turn.',
		failed_closed: 'Failed safe: the steerer evaluator errored and did not inject.'
	};

	const AUTHORITY_LABELS: Record<AuthorityCode, string> = {
		supply_context: 'Supply context',
		redirect: 'Redirect',
		command: 'Command'
	};

	const AUTHORITY_TOOLTIPS: Record<AuthorityCode, string> = {
		supply_context: 'supply_context — supply missing context/facts to the session.',
		redirect: 'redirect — call out the drift and point attention back to the thread.',
		command: 'command — issue a direct instruction.'
	};

	const MODE_LABELS: Record<ModeCode, string> = {
		augment: 'Augment',
		drive: 'Drive'
	};

	const MODE_TOOLTIPS: Record<ModeCode, string> = {
		augment: 'augment — adds a nudge alongside the conversation.',
		drive: 'drive — takes a stronger, driving steering role.'
	};

	const CONFIDENCE_TOOLTIP =
		"confidence (0–1): the steerer's LLM-judge certainty that the session has drifted from its thread anchor. " +
		"The intervention only fires when confidence ≥ the steerer's confidence_threshold (typically 0.7–0.95).";

	const OUTCOME_LABELS: Record<Exclude<DispatchOutcome, null>, string> = {
		completed: 'Completed',
		abandoned: 'Abandoned',
		error: 'Errored'
	};

	const OUTCOME_TOOLTIPS: Record<Exclude<DispatchOutcome, null> | 'inflight', string> = {
		completed: 'The agent sub-task finished and reported back.',
		abandoned: 'The agent sub-task was cancelled or did not report back.',
		error: 'The agent sub-task terminated with an error.',
		inflight: 'The agent sub-task is still running — no outcome yet.'
	};

	const MODEL_TOOLTIPS: Record<string, string> = {
		'claude-sonnet-4-6':
			'claude-sonnet-4-6 — Anthropic Sonnet 4.6. Balanced performance and speed.',
		'claude-haiku-4-5-20251001':
			'claude-haiku-4-5-20251001 — Anthropic Haiku 4.5. Fast and cost-effective.'
	};

	const AGENT_SLOT: Record<string, string> = {
		'rust-developer': '1',
		'frontend-developer': '2',
		'ui-designer': '3',
		'db-dev': '4',
		watchdog: '5'
	};

	interface Props {
		steerers: SteererEntry[];
		agents: AgentEntry[];
		onJumpTo: (requestId: number) => void;
		embedded?: boolean;
	}

	let { steerers, agents, onJumpTo, embedded = false }: Props = $props();

	let steerersGroupExpanded = $state(true);
	let agentsGroupExpanded = $state(true);

	let steererExpanded = $state<boolean[]>([]);
	let agentExpanded = $state<boolean[]>([]);

	$effect(() => {
		steererExpanded = steerers.map(() => false);
	});

	$effect(() => {
		agentExpanded = agents.map(() => false);
	});

	function modelTooltip(model: string): string {
		return MODEL_TOOLTIPS[model] ?? model;
	}

	function agentFg(type: string): string {
		const slot = AGENT_SLOT[type];
		return slot ? `var(--agent-${slot}-fg)` : 'var(--text-faint)';
	}

	function steererStatusLabel(steerer: SteererEntry): string {
		if (steerer.events.length === 0) {
			return steerer.enrolled ? 'enrolled — not fired yet' : 'no evaluations recorded';
		}
		const fired = steerer.events.filter((e) => e.verdict === 'fired').length;
		const evalWord = steerer.events.length === 1 ? 'evaluation' : 'evaluations';
		return fired > 0
			? `${steerer.events.length} ${evalWord} · ${fired} fired`
			: `${steerer.events.length} ${evalWord}`;
	}

	function verdictStyle(v: VerdictCode): string {
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
		}
	}

	function outcomeStyle(outcome: DispatchOutcome): string {
		switch (outcome) {
			case 'completed':
				return 'color: var(--status-success-fg); background: var(--status-success-bg); border-color: var(--status-success-border);';
			case 'abandoned':
				return 'color: var(--text-2); background: var(--surface-2); border-color: var(--border-subtle);';
			case 'error':
				return 'color: var(--status-danger-fg); background: var(--status-danger-bg); border-color: var(--status-danger-border);';
			case null:
				return 'color: var(--status-info-fg); background: var(--surface-2); border-color: var(--border-subtle);';
		}
	}

	function formatTimestamp(iso: string): string {
		try {
			return new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
		} catch {
			return iso;
		}
	}
</script>

<div class="asb-band" class:asb-band--embedded={embedded}>
	<section class="asb-group asb-group--steerers" class:asb-group--embedded={embedded}>
		<div class="asb-group-hd">
			<h3 class="asb-steerers-hd panel-subhd">Steerers</h3>
			<span class="asb-group-count">{steerers.length}</span>
			<span class="asb-group-spacer" aria-hidden="true"></span>
			<button
				type="button"
				class="asb-group-toggle"
				data-motion="hover"
				aria-expanded={steerersGroupExpanded}
				onclick={() => (steerersGroupExpanded = !steerersGroupExpanded)}
				aria-label={steerersGroupExpanded ? 'Collapse steerers' : 'Expand steerers'}
			>
				<DisclosureMarker open={steerersGroupExpanded} size={12} />
			</button>
		</div>

		<p class="asb-group-explainer">
			Background evaluators that watch the session for drift — such as a false "done" claim or scope
			deviation — and nudge the agent on the next turn. They run after every response with no added
			latency. Each steerer below shows how many times it evaluated and whether it fired.
		</p>

		{#if steerersGroupExpanded}
			{#if steerers.length === 0}
				<p class="asb-empty">No steerers enrolled or active in this session.</p>
			{:else}
				<div class="asb-entries asb-entries--steerers">
					{#each steerers as steerer, i (steerer.steererName ?? i)}
						{@const hasFired = steerer.events.some((e) => e.verdict === 'fired')}
						<div class="asb-entry-elev" data-elevation="card" data-motion="hover">
							<div class="asb-entry asb-entry--steerer" data-augmented-ui="tr-clip br-clip border">
								<div
									class="asb-entry__rail"
									style="background: {hasFired
										? 'var(--status-steering-fg)'
										: 'var(--border-subtle)'};"
									aria-hidden="true"
								></div>
								<div class="asb-entry__body">
									<div class="asb-entry__head">
										<span class="asb-steerer-name entity-name">
											{steerer.steererDisplayName ?? steerer.steererName ?? 'unknown'}
										</span>
										{#if steerer.enrolled}
											<span class="asb-tag asb-tag--enrolled">Enrolled</span>
										{/if}
										{#if steerer.authority}
											<abbr class="asb-tag" title={AUTHORITY_TOOLTIPS[steerer.authority]}>
												{AUTHORITY_LABELS[steerer.authority]}
											</abbr>
										{/if}
										{#if steerer.mode}
											<abbr class="asb-tag" title={MODE_TOOLTIPS[steerer.mode]}>
												{MODE_LABELS[steerer.mode]}
											</abbr>
										{/if}
										<span class="asb-spacer" aria-hidden="true"></span>
										<span class="asb-event-tally">{steererStatusLabel(steerer)}</span>
									</div>

									{#if steerer.description}
										<p class="asb-steerer-desc">{steerer.description}</p>
									{:else}
										<p class="asb-steerer-desc asb-steerer-desc--missing">
											<em>No description — requires SteererDto join by steerer_name.</em>
										</p>
									{/if}

									{#if steerer.events.length > 0}
										<button
											type="button"
											class="asb-expand-btn"
											data-motion="hover"
											aria-expanded={steererExpanded[i]}
											onclick={() => {
												steererExpanded[i] = !steererExpanded[i];
											}}
										>
											<DisclosureMarker open={steererExpanded[i] ?? false} size={11} />
											{#if steererExpanded[i]}
												Hide events
											{:else}
												Show events ({steerer.events.length})
											{/if}
										</button>
									{/if}

									{#if steererExpanded[i]}
										<div
											class="asb-timeline"
											role="list"
											aria-label="Evaluation history for {steerer.steererDisplayName}"
										>
											{#each steerer.events as event (event.createdAt)}
												{@const jumpTarget = event.sentRequestId ?? event.triggerRequestId}
												<div class="asb-event asb-event--{event.verdict}" role="listitem">
													<span class="asb-event-time">{formatTimestamp(event.createdAt)}</span>

													<abbr
														class="asb-event-chip"
														style={verdictStyle(event.verdict)}
														title={VERDICT_TOOLTIPS[event.verdict]}
													>
														{VERDICT_LABELS[event.verdict]}
													</abbr>

													{#if event.confidence !== null}
														<abbr class="asb-event-conf" title={CONFIDENCE_TOOLTIP}>
															confidence {event.confidence.toFixed(2)}
														</abbr>
													{/if}

													{#if event.gateReason}
														<span class="asb-event-reason">{event.gateReason}</span>
													{/if}

													<span class="asb-spacer" aria-hidden="true"></span>

													{#if jumpTarget !== null}
														<button
															type="button"
															class="asb-event-jump"
															data-motion="hover"
															onclick={() => onJumpTo(jumpTarget)}
															aria-label="Jump to request #{jumpTarget}"
														>
															<ArrowUpRight size={10} aria-hidden="true" /> #{jumpTarget}
														</button>
													{/if}

													{#if event.payloadText}
														<div class="asb-payload-wrap">
															<p class="asb-payload-label">What it did:</p>
															<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
															<pre
																class="asb-payload"
																tabindex="0"
																role="region"
																aria-label="Payload for {steerer.steererDisplayName ??
																	steerer.steererName ??
																	'unknown'}">{event.payloadText}</pre>
														</div>
													{/if}
												</div>
											{/each}
										</div>
									{/if}
								</div>
							</div>
						</div>
					{/each}
				</div>
			{/if}
		{/if}
	</section>

	<div class="asb-sep" aria-hidden="true"></div>

	<section class="asb-group asb-group--agents" class:asb-group--embedded={embedded}>
		<div class="asb-group-hd">
			<h3 class="asb-agents-hd panel-subhd">Sub-agents</h3>
			<span class="asb-group-count">{agents.length}</span>
			<span class="asb-group-spacer" aria-hidden="true"></span>
			<button
				type="button"
				class="asb-group-toggle"
				data-motion="hover"
				aria-expanded={agentsGroupExpanded}
				onclick={() => (agentsGroupExpanded = !agentsGroupExpanded)}
				aria-label={agentsGroupExpanded ? 'Collapse agents' : 'Expand agents'}
			>
				<DisclosureMarker open={agentsGroupExpanded} size={12} />
			</button>
		</div>

		<p class="asb-group-explainer">
			Specialist agents dispatched via the Agent tool to handle focused sub-tasks. Each entry shows
			all the times this persona was invoked and what came of it.
		</p>

		{#if agentsGroupExpanded}
			{#if agents.length === 0}
				<p class="asb-empty">No sub-agents were dispatched in this session.</p>
			{:else}
				<div class="asb-entries asb-entries--agents">
					{#each agents as agent, i (agent.subagentType)}
						{@const fg = agentFg(agent.subagentType)}
						<div class="asb-entry-elev" data-elevation="card" data-motion="hover">
							<div class="asb-entry asb-entry--agent" data-augmented-ui="tr-clip br-clip border">
								<div class="asb-entry__rail" style="background: {fg};" aria-hidden="true"></div>
								<div class="asb-entry__body">
									<div class="asb-entry__head">
										<span class="asb-agent-type" style="color: {fg};">{agent.subagentType}</span>
										{#if agent.model}
											<abbr class="asb-model" title={modelTooltip(agent.model)}>
												{agent.model}
											</abbr>
										{/if}
										<span class="asb-spacer" aria-hidden="true"></span>
										<span class="asb-event-tally">
											{agent.events.length}
											{agent.events.length === 1 ? 'dispatch' : 'dispatches'}
										</span>
									</div>

									<p class="asb-agent-desc">{agent.description}</p>

									<button
										type="button"
										class="asb-expand-btn"
										data-motion="hover"
										aria-expanded={agentExpanded[i]}
										onclick={() => {
											agentExpanded[i] = !agentExpanded[i];
										}}
									>
										<DisclosureMarker open={agentExpanded[i] ?? false} size={11} />
										{#if agentExpanded[i]}
											Hide dispatches
										{:else}
											Show dispatches ({agent.events.length})
										{/if}
									</button>

									{#if agentExpanded[i]}
										<div
											class="asb-timeline"
											role="list"
											aria-label="Dispatch history for {agent.subagentType}"
										>
											{#each agent.events as event (event.requestId)}
												{@const outcomeKey = event.outcome}
												<div
													class="asb-event {outcomeKey
														? `asb-event--${outcomeKey}`
														: 'asb-event--inflight'}"
													role="listitem"
												>
													<span class="asb-event-time">{formatTimestamp(event.createdAt)}</span>

													<abbr
														class="asb-event-chip"
														style={outcomeStyle(event.outcome)}
														title={outcomeKey
															? OUTCOME_TOOLTIPS[outcomeKey]
															: OUTCOME_TOOLTIPS['inflight']}
													>
														{outcomeKey ? OUTCOME_LABELS[outcomeKey] : 'In progress'}
													</abbr>

													{#if event.description}
														<span class="asb-event-reason">{event.description}</span>
													{/if}

													<span class="asb-spacer" aria-hidden="true"></span>

													<button
														type="button"
														class="asb-event-jump"
														data-motion="hover"
														onclick={() => onJumpTo(event.requestId)}
														aria-label="Jump to request #{event.requestId}"
													>
														<ArrowUpRight size={10} aria-hidden="true" /> #{event.requestId}
													</button>
												</div>
											{/each}
										</div>
									{/if}
								</div>
							</div>
						</div>
					{/each}
				</div>
			{/if}
		{/if}
	</section>
</div>

<style>
	.asb-band {
		background: var(--surface-bg);
		border-top: 1px solid var(--border-subtle);
		border-bottom: 1px solid var(--border-subtle);
	}

	.asb-band--embedded {
		border-bottom: none;
	}

	.asb-group {
		padding: 16px 20px;
	}

	.asb-group--embedded {
		padding: 14px 16px;
	}

	.asb-group-hd {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-bottom: 6px;
	}

	.asb-steerers-hd,
	.asb-agents-hd {
		margin: 0;
	}

	.asb-group-count {
		padding: 0 5px;
		background: var(--surface-2);
		border-radius: 2px;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-2);
	}

	.asb-group-spacer {
		flex: 1 1 auto;
	}

	.asb-group-toggle {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 20px;
		height: 20px;
		border: none;
		background: transparent;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--text-2);
		cursor: pointer;

		flex-shrink: 0;
	}

	.asb-group-toggle:hover {
		color: var(--text-strong);
	}

	.asb-group-toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.asb-group-explainer {
		margin: 0 0 14px;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		line-height: 1.55;
		color: var(--text-2);
		max-width: 72ch;
		overflow-wrap: break-word;
	}

	.asb-sep {
		border-top: 1px solid var(--border-default);
	}

	.asb-entries {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.asb-entry {
		--edge: 5px;
		--aug-tr: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		display: flex;
		background: var(--surface-card);
		overflow: hidden;
		box-shadow: inset 0 1px 0 var(--surface-shine);
	}

	.asb-entry__rail {
		flex: 0 0 3px;
	}

	.asb-entry__body {
		flex: 1 1 auto;
		padding: 10px 12px;
		display: flex;
		flex-direction: column;
		gap: 5px;
		min-width: 0;
	}

	.asb-entry__head {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px;
	}

	.asb-steerer-name {
		color: var(--text-strong);
	}

	.asb-tag {
		padding: 1px 6px;
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-2);
		background: var(--surface-2);
		text-decoration: none;
		border-bottom: 1px dotted var(--border-default);
		cursor: help;
	}

	.asb-tag--enrolled {
		color: var(--status-steering-fg);
		background: var(--status-steering-bg);
		border-color: var(--status-steering-border);
		border-bottom-style: solid;
		text-transform: uppercase;
		font-weight: 600;
		cursor: default;
	}

	.asb-agent-type {
		font-family: var(--font-mono, monospace);
		font-size: 13px;
		font-weight: 600;
	}

	.asb-model {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-2);
		background: var(--surface-2);
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		padding: 0 5px;
		text-decoration: none;
		border-bottom: 1px dotted var(--border-default);
		cursor: help;
	}

	.asb-spacer {
		flex: 1 1 auto;
	}

	.asb-event-tally {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
	}

	.asb-steerer-desc,
	.asb-agent-desc {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		line-height: 1.5;
		color: var(--text-1);
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.asb-steerer-desc--missing {
		color: var(--text-3);
	}

	.asb-expand-btn {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 2px 0;
		border: none;
		background: transparent;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-2);
		cursor: pointer;
		align-self: flex-start;
	}

	.asb-expand-btn:hover {
		color: var(--text-strong);
	}

	.asb-expand-btn:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.asb-timeline {
		display: flex;
		flex-direction: column;
		gap: 0;
		border-top: 1px solid var(--border-subtle);
		padding-top: 6px;
		margin-top: 2px;
	}

	.asb-event {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 6px;
		padding: 5px 0;
		border-bottom: 1px solid oklch(from var(--border-subtle) l c h / 0.4);
	}

	.asb-event:last-child {
		border-bottom: none;
	}

	.asb-event--fired {
		box-shadow: inset 3px 0 0 0 var(--status-steering-fg);
		padding-left: 6px;
	}

	.asb-event-time {
		flex: 0 0 44px;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-3);
		font-variant-numeric: tabular-nums;
		align-self: center;
	}

	.asb-event-chip {
		padding: 1px 5px;
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 600;
		white-space: nowrap;
		flex-shrink: 0;
		text-decoration: none;
		border-bottom-style: dotted;
		cursor: help;
		align-self: center;
	}

	.asb-event-conf {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-3);
		flex-shrink: 0;
		text-decoration: none;
		border-bottom: 1px dotted var(--border-default);
		cursor: help;
		align-self: center;
	}

	.asb-event-reason {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-1);
		overflow-wrap: break-word;
		word-break: break-word;
		flex: 1 1 200px;
	}

	.asb-event-jump {
		display: inline-flex;
		align-items: center;
		gap: 2px;
		padding: 1px 5px;
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		background: transparent;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--status-info-fg);
		font-weight: 600;
		cursor: pointer;
		flex-shrink: 0;
		align-self: center;
	}

	.asb-event-jump:hover {
		background: var(--interactive-hover-surface);
		border-color: var(--border-default);
	}

	.asb-event-jump:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 1px;
	}

	.asb-payload-wrap {
		flex: 0 0 100%;
		display: flex;
		flex-direction: column;
		gap: 3px;
		padding-top: 4px;
	}

	.asb-payload-label {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--status-steering-fg);
	}

	.asb-payload {
		margin: 0;
		max-height: 200px;
		overflow-y: auto;
		padding: 8px 10px;
		background: var(--surface-2);
		border: 1px solid var(--status-steering-border, var(--border-subtle));
		border-left: 3px solid var(--status-steering-fg);
		border-radius: 2px;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-1);
		line-height: 1.5;
		white-space: pre-wrap;
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.asb-empty {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
	}
</style>
