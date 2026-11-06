<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, fn } from 'storybook/test';
	import AgentsSteerersBand from './AgentsSteerersBand.svelte';
	import 'augmented-ui/augmented-ui.min.css';

	type VerdictCode =
		| 'fired'
		| 'suppressed_debounce'
		| 'suppressed_arbitration'
		| 'no_drift'
		| 'gate_miss'
		| 'failed_closed';

	type AuthorityCode = 'supply_context' | 'redirect' | 'command';
	type ModeCode = 'augment' | 'drive';
	type DispatchOutcome = 'completed' | 'abandoned' | 'error' | null;

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

	function modelTooltip(model: string): string {
		return MODEL_TOOLTIPS[model] ?? model;
	}

	interface SteererEvent {
		createdAt: string;
		verdict: VerdictCode;
		confidence: number | null;
		gateReason: string | null;
		payloadText: string | null;
		triggerRequestId: number | null;
		sentRequestId: number | null;
	}

	interface SteererEntry {
		steererDisplayName: string | null;
		steererName: string | null;
		description: string | null;
		authority: AuthorityCode | null;
		mode: ModeCode | null;
		enrolled: boolean;
		events: SteererEvent[];
	}

	interface AgentEvent {
		requestId: number;
		createdAt: string;
		outcome: DispatchOutcome;
		description: string | null;
	}

	interface AgentEntry {
		subagentType: string;
		model: string | null;
		description: string;
		events: AgentEvent[];
	}

	const STEERERS: SteererEntry[] = [
		{
			steererDisplayName: 'VerifierSteerer',
			steererName: 'verifier',
			description:
				'Watches for completion claims ("done", "fixed", "should work") when the evidence — ' +
				'test runs, committed files, build output — is absent from the same turn. ' +
				'Fires a redirect nudge on the next turn.',
			authority: 'redirect',
			mode: 'augment',
			enrolled: true,
			events: [
				{
					createdAt: '2026-06-25T09:41:12Z',
					verdict: 'fired',
					confidence: 0.91,
					gateReason:
						'The assistant claimed completion but the test suite was not run ' +
						'and the changed file was not committed. Fired to redirect to the verification step.',
					payloadText:
						'[bigbro steerer] VerifierSteerer detected a verifier pattern.\n\n' +
						'Current state: You claimed the task is complete, but:\n' +
						'- No test run output was produced\n' +
						'- The changed file (crates/mike/src/auth/jwt.rs) is uncommitted\n\n' +
						'Next steps:\n' +
						'(1) Run `cargo nextest run -p mike --test auth` and paste the output\n' +
						'(2) Commit the file with a descriptive message\n\n' +
						'Please confirm alignment before proceeding.',
					triggerRequestId: 1807,
					sentRequestId: 1808
				},
				{
					createdAt: '2026-06-25T09:28:55Z',
					verdict: 'suppressed_debounce',
					confidence: 0.63,
					gateReason: null,
					payloadText: null,
					triggerRequestId: 1791,
					sentRequestId: null
				},
				{
					createdAt: '2026-06-25T09:14:03Z',
					verdict: 'no_drift',
					confidence: 0.22,
					gateReason: null,
					payloadText: null,
					triggerRequestId: 1762,
					sentRequestId: null
				}
			]
		},
		{
			steererDisplayName: 'ScopeGuard',
			steererName: 'scope_guard',
			description:
				'Detects when the assistant operates on files outside the declared allowlist or ' +
				'creates artefacts not in the plan. Fires a command injection to halt and surface the scope deviation.',
			authority: 'command',
			mode: 'drive',
			enrolled: false,
			events: [
				{
					createdAt: '2026-06-25T09:14:03Z',
					verdict: 'no_drift',
					confidence: 0.12,
					gateReason: null,
					payloadText: null,
					triggerRequestId: 1762,
					sentRequestId: null
				}
			]
		},
		{
			steererDisplayName: 'ThreadAnchorGuard',
			steererName: 'thread_anchor_guard',
			description:
				'Watches for the session drifting away from its declared thread anchor without an ' +
				'explicit scope change. Enrolled on this session but has not evaluated yet — the ' +
				'evaluator runs on the next response.',
			authority: 'supply_context',
			mode: 'augment',
			enrolled: true,
			events: []
		}
	];

	const AGENTS: AgentEntry[] = [
		{
			subagentType: 'rust-developer',
			model: 'claude-sonnet-4-6',
			description:
				'Implement the JWT middleware refactor: rewrite `crates/mike/src/auth/jwt.rs` ' +
				'to use HMAC-SHA256 with a 1-hour sliding window. Update the `AuthGuard` extractor ' +
				'to short-circuit on bearer-token absence and return 401 with a structured JSON error body. ' +
				'Add integration tests covering the new guard path.',
			events: [
				{
					requestId: 2089,
					createdAt: '2026-06-25T09:52:18Z',
					outcome: 'completed',
					description:
						'Re-run: add integration tests covering the new AuthGuard extractor path. All 14 tests must pass.'
				},
				{
					requestId: 2042,
					createdAt: '2026-06-25T09:38:44Z',
					outcome: 'abandoned',
					description:
						'Implement the JWT middleware refactor and add integration tests covering the new AuthGuard extractor path.'
				}
			]
		},
		{
			subagentType: 'frontend-developer',
			model: 'claude-haiku-4-5-20251001',
			description:
				'Wire the refresh-token flow into authStore.svelte.ts. The store should ' +
				'auto-refresh 60 seconds before token expiry using a $effect-based timer.',
			events: [
				{
					requestId: 2101,
					createdAt: '2026-06-25T10:01:44Z',
					outcome: null,
					description: 'Wire the refresh-token flow into authStore.svelte.ts with a $effect timer.'
				}
			]
		}
	];

	const AGENT_SLOT: Record<string, string> = {
		'rust-developer': '1',
		'frontend-developer': '2',
		'ui-designer': '3',
		'db-dev': '4',
		watchdog: '5'
	};

	function agentFg(type: string): string {
		const slot = AGENT_SLOT[type];
		return slot ? `var(--agent-${slot}-fg)` : 'var(--text-faint)';
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

	const onJumpTo = fn();

	const { Story } = defineMeta({
		title: 'Shell/AgentsSteerersBand',
		component: AgentsSteerersBand,
		tags: ['autodocs'],
		parameters: {
			layout: 'fullscreen',
			docs: {
				description: {
					component:
						'Full-width stacked band: Steerers group on top (with explainer), Agents below. ' +
						'Groups collapse/expand as a whole (v3). ' +
						'Each actor has an expandable mini-timeline of ALL invocation events. ' +
						'Tooltips on authority, mode, verdict, confidence, outcome, model. ' +
						'Fired events show payload_text inline. ' +
						'DATA GAP: steerer description requires SteererDto join. ' +
						'Design brief: artifacts/2026-06-25/graders-ui-design.md §4 v3.'
				}
			}
		}
	});
</script>

<Story
	name="WithAgentsAndSteerers"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const band = canvasElement.querySelector('.asb-band') as HTMLElement;
		const elev = canvasElement.querySelector('.asb-entry-elev') as HTMLElement;
		const entry = elev?.querySelector('.asb-entry') as HTMLElement;
		await expect(elev).not.toBeNull();
		await expect(entry).not.toBeNull();
		await expect(elev).toHaveAttribute('data-elevation', 'card');

		const shadowProbe = document.createElement('div');
		shadowProbe.style.boxShadow = 'var(--shadow-sm)';
		document.body.appendChild(shadowProbe);
		const cardShadow = getComputedStyle(shadowProbe).boxShadow;
		shadowProbe.remove();
		await expect(cardShadow).not.toBe('none');
		await expect(getComputedStyle(elev).boxShadow).toBe(cardShadow);
		const entryLayers = getComputedStyle(entry)
			.boxShadow.split(/,(?![^()]*\))/)
			.map((layer) => layer.trim())
			.filter(Boolean);
		await expect(entryLayers.length).toBeGreaterThan(0);
		for (const layer of entryLayers) {
			await expect(layer, `${layer} is an outer shadow on a clipped element`).toContain('inset');
		}

		await expect(getComputedStyle(entry).backgroundColor).not.toBe(
			getComputedStyle(band).backgroundColor
		);

		await expect(canvasElement.querySelector('.asb-steerers-hd')).not.toBeNull();

		await expect(canvasElement.querySelector('.asb-agents-hd')).not.toBeNull();

		const allGroups = canvasElement.querySelectorAll('.asb-group');
		await expect(allGroups[0]!.classList.contains('asb-group--steerers')).toBe(true);
		await expect(allGroups[1]!.classList.contains('asb-group--agents')).toBe(true);

		await expect(canvasElement.querySelector('.asb-entries--steerers')).not.toBeNull();

		const groupToggles = canvas.getAllByRole('button', {
			name: /collapse steerers|expand steerers/i
		});
		await userEvent.click(groupToggles[0]!);
		await expect(canvasElement.querySelector('.asb-entries--steerers')).toBeNull();
		await userEvent.click(groupToggles[0]!);

		const expandBtns = canvas.getAllByRole('button', { name: /show events|hide events/i });
		await userEvent.click(expandBtns[0]!);

		const events = canvasElement.querySelectorAll('.asb-event');
		await expect(events.length).toBeGreaterThanOrEqual(3);

		const firedEvents = canvasElement.querySelectorAll('.asb-event--fired');
		await expect(firedEvents.length).toBeGreaterThan(0);
		const verdictChip = firedEvents[0]!.querySelector('.asb-event-chip');
		await expect(verdictChip?.textContent?.trim()).toBe('Fired');

		const payload = firedEvents[0]!.querySelector('.asb-payload');
		await expect(payload).not.toBeNull();
		await expect(payload?.textContent).toContain('[bigbro steerer]');

		const heldBack = canvasElement.querySelector('.asb-event--suppressed_debounce');
		await expect(heldBack).not.toBeNull();
		await expect(heldBack!.querySelector('.asb-event-jump')).not.toBeNull();

		const jumpBtns = canvas.getAllByRole('button', { name: /jump to/i });
		await userEvent.click(jumpBtns[0]!);
		await expect(onJumpTo).toHaveBeenCalledWith(1808);

		const enrolledTags = canvasElement.querySelectorAll('.asb-tag--enrolled');
		await expect(enrolledTags.length).toBe(2);

		const steererCards = canvasElement.querySelectorAll('.asb-entry--steerer');
		await expect(steererCards.length).toBe(3);

		const verifierTally = steererCards[0]!.querySelector('.asb-event-tally');
		await expect(verifierTally?.textContent?.trim()).toBe('3 evaluations · 1 fired');

		await expect(steererCards[1]!.querySelector('.asb-tag--enrolled')).toBeNull();
		const scopeGuardTally = steererCards[1]!.querySelector('.asb-event-tally');
		await expect(scopeGuardTally?.textContent?.trim()).toBe('1 evaluation');

		await expect(steererCards[2]!.querySelector('.asb-tag--enrolled')).not.toBeNull();
		const anchorGuardTally = steererCards[2]!.querySelector('.asb-event-tally');
		await expect(anchorGuardTally?.textContent?.trim()).toBe('enrolled — not fired yet');
		await expect(
			within(steererCards[2] as HTMLElement).queryByRole('button', { name: /show events/i })
		).toBeNull();
	}}
>
	{#snippet template()}
		<div style="background: var(--surface-bg); padding: 0; width: 100%;">
			<AgentsSteerersBand steerers={STEERERS} agents={AGENTS} {onJumpTo} />
		</div>
	{/snippet}
</Story>
