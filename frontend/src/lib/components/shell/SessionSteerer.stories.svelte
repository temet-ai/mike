<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, fn } from 'storybook/test';
	import SessionSteerer from './SessionSteerer.svelte';
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

	interface SteererEvent {
		createdAt: string;
		verdict: VerdictCode;
		confidence: number | null;
		gateReason: string | null;
		payloadText: string | null;
		triggerRequestId: number | null;
		sentRequestId: number | null;
	}

	interface SteererData {
		steererDisplayName: string | null;
		steererName: string | null;
		description: string | null;
		authority: AuthorityCode | null;
		mode: ModeCode | null;
		events: SteererEvent[];
	}

	const STEERER_DESCRIPTIONS: Record<string, string> = {
		verifier:
			'Watches for completion claims ("done", "fixed", "should work") when the evidence — ' +
			'test runs, committed files, build output — is absent from the same turn. ' +
			'Fires a redirect nudge on the next turn.'
	};

	const STEERER_FIXTURE: SteererData = {
		steererDisplayName: 'VerifierSteerer',
		steererName: 'verifier',
		description: STEERER_DESCRIPTIONS['verifier'] ?? null,
		authority: 'redirect',
		mode: 'augment',
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
					'(2) Commit the file with a descriptive message\n' +
					'(3) Confirm completion with the test output as evidence\n\n' +
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
	};

	function formatTimestamp(iso: string): string {
		try {
			return new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
		} catch {
			return iso;
		}
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

	const onJumpTo = fn();

	const { Story } = defineMeta({
		title: 'Shell/SessionSteerer',
		component: SessionSteerer,
		tags: ['autodocs'],
		parameters: {
			docs: {
				description: {
					component:
						'One steerer entry: purpose description, authority/mode with tooltips, ' +
						'expandable mini-timeline of ALL events (fired + held-back + no-drift). ' +
						'Fired events show payload_text inline (scrollable pre viewport). ' +
						'Tooltips sourced verbatim from design brief GLOSSARY (steerer code). ' +
						'DATA GAP: description from SteererDto (fleet config), not SessionSteeringRow. ' +
						'Design brief: artifacts/2026-06-25/graders-ui-design.md §3 v3.'
				}
			}
		}
	});
</script>

<Story
	name="FiredSteerer"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const nameEl = canvasElement.querySelector('.ss-name');
		await expect(nameEl).not.toBeNull();
		await expect(nameEl?.textContent?.trim()).toBe('VerifierSteerer');

		const descEl = canvasElement.querySelector('.ss-desc');
		await expect(descEl).not.toBeNull();
		await expect(descEl?.textContent).toContain('completion claims');

		const expandBtn = canvas.getByRole('button', {
			name: /show interventions|hide interventions/i
		});
		await userEvent.click(expandBtn);

		const events = canvasElement.querySelectorAll('.ss-event');
		await expect(events.length).toBe(3);

		const firedEvent = canvasElement.querySelector('.ss-event--fired');
		await expect(firedEvent).not.toBeNull();
		await expect(firedEvent!.querySelector('.ss-event-verdict')?.textContent?.trim()).toBe('Fired');

		const payload = firedEvent!.querySelector('.ss-payload');
		await expect(payload).not.toBeNull();
		await expect(payload?.textContent).toContain('[bigbro steerer]');

		const heldBackEvent = canvasElement.querySelector('.ss-event--suppressed_debounce');
		await expect(heldBackEvent).not.toBeNull();
		await expect(heldBackEvent!.querySelector('.ss-event-jump')).not.toBeNull();

		const confEl = firedEvent!.querySelector('.ss-conf');
		await expect(confEl?.textContent).toContain('confidence');
		await expect(confEl?.textContent).toContain('0.91');

		const jumpBtns = canvas.getAllByRole('button', { name: /jump to/i });
		await userEvent.click(jumpBtns[0]!);
		await expect(onJumpTo).toHaveBeenCalledWith(1808);
	}}
>
	{#snippet template()}
		<div style="background: var(--surface-bg); padding: 16px; width: 100%;">
			<SessionSteerer steerer={STEERER_FIXTURE} {onJumpTo} />
		</div>
	{/snippet}
</Story>
