<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, fn } from 'storybook/test';
	import SessionMechanismsInventory from './SessionMechanismsInventory.svelte';
	import 'augmented-ui/augmented-ui.min.css';
	import type {
		AuthorityCode,
		ModeCode,
		FilterRuleKind,
		SteererRow,
		GraderRow,
		FilterRuleRow,
		AnalyzerRow
	} from './SessionMechanismsInventory.svelte';

	const STEERERS: SteererRow[] = [
		{
			steerer_id: 1,
			display_name: 'VerifierSteerer',
			name: 'verifier',
			description:
				'Watches for completion claims ("done", "fixed", "should work") when the evidence — ' +
				'test runs, committed files, build output — is absent from the same turn. ' +
				'Fires a redirect nudge on the next turn.',
			authority: 'redirect',
			mode: 'augment',
			model: 'claude-haiku-4-5-20251001',
			kind: 'verifier',
			enabled: true,
			enrolment_required: false,
			enrolment: null,
			activity: { fired: 1, held: 2 }
		},
		{
			steerer_id: 2,
			display_name: 'ScopeGuard',
			name: 'scope_guard',
			description:
				'Detects when the assistant operates on files outside the declared allowlist or ' +
				'creates artefacts not in the plan. Fires a command injection to halt and ' +
				'surface the scope deviation.',
			authority: 'command',
			mode: 'drive',
			model: 'claude-haiku-4-5-20251001',
			kind: 'refocus',
			enabled: true,
			enrolment_required: true,
			enrolment: {
				enrolment_id: 13,
				enrolled_at: '2026-06-26T09:15:00Z',
				source: 'manual'
			},
			activity: { fired: 0, held: 0 }
		},
		{
			steerer_id: 3,
			display_name: 'ContextInjector',
			name: 'context_injector',
			description:
				'Monitors for turns where the assistant lacks key context about the codebase ' +
				'architecture. Injects the relevant context so the agent proceeds with ' +
				'accurate assumptions.',
			authority: 'supply_context',
			mode: 'augment',
			model: 'claude-haiku-4-5-20251001',
			kind: 'refocus',
			enabled: true,
			enrolment_required: true,
			enrolment: null,
			activity: { fired: 0, held: 0 }
		}
	];

	const GRADERS: GraderRow[] = [
		{
			grader_name: 'verifier_grader',
			display_name: 'VerifierGrader',
			enabled: true,
			this_session: { turns_graded: 3, corrections: 1, accepted: 2 }
		}
	];

	const FILTER_RULES: FilterRuleRow[] = [
		{
			rule_id: 55,
			kind: 'text_substring',
			label: 'Verification discipline — completion claim gate',
			tag: 'verification-discipline',
			enabled: true,
			fired_turns: 4
		},
		{
			rule_id: 57,
			kind: 'text_substring',
			label: 'Subagent scope discipline — Agent SDK context',
			tag: 'verification-discipline',
			enabled: true,
			fired_turns: 2
		}
	];

	const ANALYZERS: AnalyzerRow[] = [
		{
			analyzer_name: 'overselling',
			display_name: 'Overselling',
			enabled: true,
			hit_count: 2,
			is_intercept: false
		},
		{
			analyzer_name: 'avoiding_work',
			display_name: 'Avoiding work',
			enabled: true,
			hit_count: 0,
			is_intercept: false
		},
		{
			analyzer_name: 'weasel_intercept',
			display_name: 'Weasel intercept',
			enabled: true,
			hit_count: 0,
			is_intercept: true
		},
		{
			analyzer_name: 'verifier_analysis',
			display_name: 'Verifier (→ VerifierGrader)',
			enabled: false,
			hit_count: 0,
			is_intercept: false
		}
	];

	let sessionActive = $state(true);

	const onEnrol = fn();
	const onDisenrol = fn();
	const onToggleSteerer = fn();

	const { Story } = defineMeta({
		title: 'Shell/SessionMechanismsInventory',
		component: SessionMechanismsInventory,
		tags: ['autodocs'],
		parameters: {
			layout: 'fullscreen',
			docs: {
				description: {
					component:
						'At-a-glance inventory of every mechanism that affects the session: steerers, ' +
						'graders, filter rules, and analyzers. Sits at the TOP of the session page as a ' +
						'collapsible settings disclosure (collapsed by default). ' +
						'Every mechanism listed even when inactive — greyed, never hidden. ' +
						'Steerers section carries the enrol/disenrol steer controls (gated on isActive). ' +
						'"Add steerer" is section-scoped (inside the Steerers section footer). ' +
						'All DATA GAPS require backend work before wiring to live data. ' +
						'See artifacts/2026-06-26/session-hub-steer-design.md.'
				}
			}
		}
	});
</script>

<Story
	name="WithLiveMechanisms"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const topToggle = canvasElement.querySelector('.smi-top-toggle');
		await expect(topToggle).not.toBeNull();

		await expect(canvasElement.querySelector('.smi-panel')).not.toBeNull();

		await userEvent.click(topToggle!);
		await expect(canvasElement.querySelector('.smi-panel')).toBeNull();
		await expect(canvasElement.querySelector('.smi-top-summary')).not.toBeNull();

		await userEvent.click(topToggle!);
		await expect(canvasElement.querySelector('.smi-panel')).not.toBeNull();

		await expect(canvasElement.querySelector('.smi-hd')).not.toBeNull();

		const sectionBtns = canvasElement.querySelectorAll('.smi-section-btn');
		await expect(sectionBtns.length).toBe(4);

		const steerersBody = canvasElement.querySelector('.smi-section-body[data-section="steerers"]');
		await expect(steerersBody).not.toBeNull();
		await expect(steerersBody!.getAttribute('aria-hidden')).toBe('false');

		const fdRow = canvasElement.querySelector('.smi-row--verifier');
		await expect(fdRow).not.toBeNull();
		await expect(fdRow!.querySelector('.smi-fd-badge')).not.toBeNull();
		const globalToggle = fdRow!.querySelector('.smi-global-toggle') as HTMLButtonElement | null;
		await expect(globalToggle).not.toBeNull();
		await expect(globalToggle!.getAttribute('aria-checked')).toBe('true');
		const fdCounts = fdRow!.querySelector('.smi-fd-counts');
		await expect(fdCounts).not.toBeNull();
		await expect(fdCounts!.textContent).toContain('3');
		await expect(fdCounts!.textContent).toContain('2');
		await expect(fdRow!.querySelector('.smi-enrol-btn')).toBeNull();
		await expect(fdRow!.querySelector('.smi-disenrol-btn')).toBeNull();

		const enrolledRow = canvasElement.querySelector('.smi-row--enrolled');
		await expect(enrolledRow).not.toBeNull();
		await expect(enrolledRow!.querySelector('.smi-source--manual')?.textContent?.trim()).toBe(
			'manual'
		);

		const notEnrolledRow = canvasElement.querySelector('.smi-row--not-enrolled');
		await expect(notEnrolledRow).not.toBeNull();
		const enrolBtn = notEnrolledRow!.querySelector('.smi-enrol-btn') as HTMLButtonElement | null;
		await expect(enrolBtn).not.toBeNull();
		await expect(enrolBtn!.disabled).toBe(false);

		await userEvent.click(enrolBtn!);
		await expect(onEnrol).toHaveBeenCalledWith(3);

		const activeSwitch = canvas.getByRole('switch', { name: /session active/i });
		await userEvent.click(activeSwitch);
		const allActionBtns = canvasElement.querySelectorAll<HTMLButtonElement>(
			'.smi-enrol-btn, .smi-disenrol-btn'
		);
		for (const btn of allActionBtns) {
			await expect(btn.disabled).toBe(true);
		}
		const globalToggleAfterFlip = canvasElement.querySelector(
			'.smi-global-toggle'
		) as HTMLButtonElement | null;
		await expect(globalToggleAfterFlip).not.toBeNull();
		await expect(globalToggleAfterFlip!.disabled).toBe(false);

		const allSteererRows = canvasElement.querySelectorAll('.smi-row--verifier, .smi-row--enrolled');
		await expect(allSteererRows.length).toBe(3);

		const graderBtn = canvasElement.querySelector(
			'.smi-section-btn[data-section="graders"]'
		) as HTMLButtonElement;
		await userEvent.click(graderBtn);
		const graderBody = canvasElement.querySelector('.smi-section-body[data-section="graders"]');
		await expect(graderBody!.getAttribute('aria-hidden')).toBe('false');
		await expect(canvasElement.querySelector('.smi-grader-row')).not.toBeNull();

		const rulesBtn = canvasElement.querySelector(
			'.smi-section-btn[data-section="rules"]'
		) as HTMLButtonElement;
		await userEvent.click(rulesBtn);
		const rulesBody = canvasElement.querySelector('.smi-section-body[data-section="rules"]');
		await expect(rulesBody!.getAttribute('aria-hidden')).toBe('false');
		const ruleRows = canvasElement.querySelectorAll('.smi-rule-row');
		await expect(ruleRows.length).toBe(2);

		const analyzersBtn = canvasElement.querySelector(
			'.smi-section-btn[data-section="analyzers"]'
		) as HTMLButtonElement;
		await userEvent.click(analyzersBtn);
		const analyzersBody = canvasElement.querySelector(
			'.smi-section-body[data-section="analyzers"]'
		);
		await expect(analyzersBody!.getAttribute('aria-hidden')).toBe('false');
		const analyzerRows = canvasElement.querySelectorAll('.smi-analyzer-row');
		await expect(analyzerRows.length).toBe(4);

		const addSteererLink = canvasElement.querySelector('.smi-section-add');
		await expect(addSteererLink).not.toBeNull();
		await expect(addSteererLink!.textContent?.trim()).toContain('Add steerer');

		const migratedChips = canvasElement.querySelectorAll(
			'.smi-badge, .smi-fd-badge, .smi-auto-badge, .smi-enrolled-chip, .smi-chip, .smi-on-chip, .smi-rule-tag'
		);
		await expect(migratedChips.length).toBeGreaterThan(0);
		for (const chip of migratedChips) {
			const style = getComputedStyle(chip);
			await expect(style.borderRadius).toBe('0px');
			await expect(chip.getAttribute('data-augmented-ui')).not.toBeNull();
			await expect(style.clipPath).not.toBe('none');
		}

		const nameEl = canvasElement.querySelector('.smi-name');
		await expect(nameEl).not.toBeNull();
		await expect(getComputedStyle(nameEl!).fontFamily.toLowerCase()).toContain('mono');
		const badgeEl = canvasElement.querySelector('.smi-badge');
		await expect(badgeEl).not.toBeNull();
		await expect(getComputedStyle(badgeEl!).fontFamily.toLowerCase()).toContain('mono');
	}}
>
	{#snippet template()}
		<div class="smi-harness">
			<span class="smi-harness__label">Harness:</span>
			<button
				type="button"
				role="switch"
				class="smi-harness__toggle"
				class:smi-harness__toggle--on={sessionActive}
				aria-label="Session active"
				aria-checked={sessionActive}
				onclick={() => {
					sessionActive = !sessionActive;
				}}
			>
				Session: {sessionActive ? 'ACTIVE' : 'INACTIVE'}
			</button>
			<span class="smi-harness__note">← toggle to verify active gate on steer controls</span>
		</div>

		<SessionMechanismsInventory
			steerers={STEERERS}
			graders={GRADERS}
			filterRules={FILTER_RULES}
			analyzers={ANALYZERS}
			{sessionActive}
			{onEnrol}
			{onDisenrol}
			{onToggleSteerer}
			verifierCounts={{ fired: 3, delivered: 2 }}
			topOpen={true}
		/>
	{/snippet}
</Story>

<Story
	name="InSessionPageContext"
	play={async ({ canvasElement }) => {
		const collapsedDemo = canvasElement.querySelector('.spc-demo-collapsed');
		await expect(collapsedDemo).not.toBeNull();
		const expandedDemo = canvasElement.querySelector('.spc-demo-expanded');
		await expect(expandedDemo).not.toBeNull();

		const collapsedToggle = collapsedDemo!.querySelector('.smi-top-toggle');
		await expect(collapsedToggle).not.toBeNull();
		await expect(collapsedDemo!.querySelector('.smi-top-summary')).not.toBeNull();

		await expect(collapsedDemo!.querySelector('.smi-panel')).toBeNull();

		await expect(expandedDemo!.querySelector('.smi-panel')).not.toBeNull();

		const sectionBtns = expandedDemo!.querySelectorAll('.smi-section-btn');
		await expect(sectionBtns.length).toBe(4);

		await expect(collapsedDemo!.querySelector('.spc-page-below')).not.toBeNull();
		await expect(expandedDemo!.querySelector('.spc-page-below')).not.toBeNull();
	}}
>
	{#snippet template()}
		<div class="spc-root">
			<div class="spc-intro">
				Session controls disclosure · in-context session-page mock · Collapsed = default state when
				you first open a session page. Expanded = full inventory (operator opens to inspect/steer).
			</div>

			<div class="spc-demo-label">Default — collapsed (one-line situational summary)</div>
			<div class="spc-demo-collapsed">
				<div class="spc-id-bar">
					<span class="spc-session-id">Session abc12345</span>
					<span class="spc-sep">·</span>
					<span class="spc-meta">47 requests</span>
					<span class="spc-sep">·</span>
					<span class="spc-live-chip">● live</span>
				</div>

				<SessionMechanismsInventory
					steerers={STEERERS}
					graders={GRADERS}
					filterRules={FILTER_RULES}
					analyzers={ANALYZERS}
					sessionActive={true}
					{onEnrol}
					{onDisenrol}
					{onToggleSteerer}
					verifierCounts={{ fired: 3, delivered: 2 }}
					topOpen={false}
				/>

				<div class="spc-page-below">
					<div class="spc-placeholder">Overview / session summary · dashboard grid</div>
					<div class="spc-placeholder spc-placeholder--tall">
						AgentsSteerersBand · turn timeline · request stream
					</div>
				</div>
			</div>

			<div class="spc-demo-label spc-demo-label--expanded">
				Expanded — full inventory (operator opened it)
			</div>
			<div class="spc-demo-expanded">
				<div class="spc-id-bar">
					<span class="spc-session-id">Session abc12345</span>
					<span class="spc-sep">·</span>
					<span class="spc-meta">47 requests</span>
					<span class="spc-sep">·</span>
					<span class="spc-live-chip">● live</span>
				</div>

				<SessionMechanismsInventory
					steerers={STEERERS}
					graders={GRADERS}
					filterRules={FILTER_RULES}
					analyzers={ANALYZERS}
					sessionActive={true}
					{onEnrol}
					{onDisenrol}
					{onToggleSteerer}
					verifierCounts={{ fired: 3, delivered: 2 }}
					topOpen={true}
				/>

				<div class="spc-page-below">
					<div class="spc-placeholder">Overview / session summary · dashboard grid</div>
					<div class="spc-placeholder">AgentsSteerersBand · turn timeline · request stream</div>
				</div>
			</div>
		</div>
	{/snippet}
</Story>

<style>
	.smi-harness {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 20px;
		background: var(--surface-2);
		border-bottom: 1px solid var(--border-subtle);
	}

	.smi-harness__label {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
	}

	.smi-harness__toggle {
		padding: 3px 10px;
		border: 1px solid var(--border-default);
		border-radius: 3px;
		background: var(--surface-card);
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-2);
		cursor: pointer;
		transition:
			background 0.12s,
			border-color 0.12s,
			color 0.12s;
	}

	.smi-harness__toggle--on {
		background: var(--status-success-bg);
		border-color: var(--status-success-border);
		color: var(--status-success-fg);
	}

	.smi-harness__toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.smi-harness__note {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-3);
	}

	.spc-root {
		background: var(--surface-bg);
		min-height: 100vh;
	}

	.spc-intro {
		padding: 8px 20px;
		background: var(--surface-2);
		border-bottom: 1px solid var(--border-subtle);
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-3);
		letter-spacing: 0.02em;
	}

	.spc-demo-label {
		padding: 6px 20px;
		background: var(--surface-2);
		border-top: 2px solid var(--border-default);
		border-bottom: 1px solid var(--border-subtle);
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.07em;
		color: var(--text-3);
	}

	.spc-demo-label--expanded {
		border-top-color: var(--status-steering-border);
	}

	.spc-id-bar {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 20px;
		background: var(--surface-card);
		border-bottom: 1px solid var(--border-subtle);
		font-family: var(--font-mono, monospace);
		font-size: 12px;
	}

	.spc-session-id {
		font-weight: 600;
		color: var(--text-1);
	}

	.spc-sep {
		color: var(--text-faint);
	}

	.spc-meta {
		color: var(--text-3);
	}

	.spc-live-chip {
		font-size: 11px;
		color: var(--status-success-fg);
	}

	.spc-page-below {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 16px 20px;
	}

	.spc-placeholder {
		padding: 20px;
		border: 1px dashed var(--border-subtle);
		border-radius: 4px;
		background: var(--surface-2);
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
		text-align: center;
	}

	.spc-placeholder--tall {
		padding: 40px;
	}
</style>
