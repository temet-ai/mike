<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';

	const { Story } = defineMeta({
		title: 'Routes/Steering/SteerAuthorFlow',
		tags: ['autodocs'],
		parameters: {
			layout: 'fullscreen',
			docs: {
				description: {
					component:
						'Steer-author UX: describe → authored anchor + per-field rationale. ' +
						'Extends the shipped Step 3 guided flow with pre-seeding from anchor_request_id ' +
						'and the steer-author agent suggestion panel. ' +
						'See artifacts/2026-06-16/weasel-steer-dryrun-spec.md §3.'
				}
			}
		}
	});

	const MOCK_PREFILL_INTENT = `Session diagnostic findings at response #42:
- [avoiding_work] The assistant claimed the migration test failure is a pre-existing issue unrelated to its changes.
- [overselling] The assistant reported "all tests pass" without running them.

Steer this session to prevent this behavior going forward.`;

	const MOCK_AUTHORED_ANCHOR = `Hold this session to the originally requested deliverable: fixing the failing migration test in tests/integration/db_test.rs.

The session MUST NOT:
- Claim that a test failure is pre-existing or out-of-scope without evidence from git history.
- Claim "all tests pass" or "tests are passing" without showing actual test output in the same turn.
- Declare a deliverable complete without a working, verified result.

Every claim of completion must be accompanied by the test output that proves it. Unverified success claims are a violation.`;

	const MOCK_RATIONALE = {
		anchor:
			'Combines your intent (fix the test) with the detected behaviors (avoiding_work, overselling) to name each forbidden pattern explicitly so the runtime steerer recognizes recurrences.',
		authority:
			'supply_context — the session is still capable of self-correcting; a context injection is the appropriate first escalation before redirect or command.',
		mode: "augment — the session generates its own turns normally; the steer injects alongside, so the operator does not override the session's reasoning.",
		confidence_threshold:
			'0.80 — raised from the default 0.70 because these are specific behavioral violations (not vague drift), so the steerer should fire only when the recurrence is clear.',
		debounce_turns:
			'1 — re-evaluate every turn because the detected patterns can recur within a single tool loop.'
	};
</script>

<Story
	name="Step3_PreSeededFromResponse"
	parameters={{
		docs: {
			description: {
				story:
					'Step 3 pre-seeded from anchor_request_id=42. Intent textarea pre-filled with diagnostic finding summaries (spec §2.3). The "Help me write this" button is active because context is available. Operator can edit the pre-fill before calling assist.'
			}
		}
	}}
>
	<div class="sb-wrapper">
		<style>
			.sb-wrapper {
				background: var(--bg, #0f1117);
				min-height: 100vh;
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-mono, monospace);
				font-size: 13px;
			}

			.gf-shell {
				display: flex;
				flex-direction: column;
				height: 100vh;
			}
			.gf-header {
				display: flex;
				align-items: center;
				gap: 12px;
				padding: 14px 20px;
				border-bottom: 1px solid var(--border-color, #30363d);
			}
			.gf-back {
				display: inline-flex;
				align-items: center;
				justify-content: center;
				width: 28px;
				height: 28px;
				border-radius: 4px;
				border: 1px solid var(--border-subtle, #23282f);
				color: var(--text-2, #adb5be);
				text-decoration: none;
				font-size: 14px;
				cursor: pointer;
				background: transparent;
			}
			.gf-title {
				font-size: 15px;
				font-weight: 700;
				color: var(--text-strong, #dde2e8);
			}
			.gf-subtitle {
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				margin-left: 4px;
			}

			.gf-steps {
				display: flex;
				align-items: center;
				gap: 0;
				padding: 0 20px;
				border-bottom: 1px solid var(--border-subtle, #23282f);
				height: 40px;
			}
			.gf-step {
				display: flex;
				align-items: center;
				gap: 6px;
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				padding: 0 12px 0 0;
			}
			.gf-step--done {
				color: var(--green, #4a9f6f);
			}
			.gf-step--active {
				color: var(--rose, #b8849e);
				font-weight: 600;
			}
			.gf-step__num {
				display: inline-flex;
				align-items: center;
				justify-content: center;
				width: 18px;
				height: 18px;
				border-radius: 50%;
				border: 1px solid;
				font-size: 10px;
				font-weight: 700;
				flex-shrink: 0;
			}
			.gf-step--done .gf-step__num {
				border-color: var(--green, #4a9f6f);
				color: var(--green, #4a9f6f);
			}
			.gf-step--active .gf-step__num {
				border-color: var(--rose, #b8849e);
				background: var(--rose-dim, rgba(184, 132, 158, 0.08));
				color: var(--rose, #b8849e);
			}
			.gf-step--idle .gf-step__num {
				border-color: var(--border-subtle, #23282f);
				color: var(--text-3, #c0c7d0);
			}
			.gf-step-sep {
				font-size: 10px;
				color: var(--text-faint, #c0c7d0);
				margin-right: 12px;
			}

			.gf-body {
				flex: 1;
				overflow-y: auto;
				padding: 24px 20px;
				display: flex;
				flex-direction: column;
				gap: 20px;
				max-width: 640px;
			}

			.gf-step-heading {
				font-size: 16px;
				font-weight: 700;
				color: var(--text-strong, #dde2e8);
				margin: 0;
			}
			.gf-step-subtext {
				font-size: 12px;
				color: var(--text-2, #adb5be);
				margin: 6px 0 0;
				line-height: 1.6;
			}

			.gf-anchor-banner {
				display: flex;
				align-items: flex-start;
				gap: 10px;
				padding: 10px 12px;
				border: 1px solid var(--rose-border, rgba(184, 132, 158, 0.15));
				border-radius: 6px;
				background: var(--rose-dim, rgba(184, 132, 158, 0.08));
			}
			.gf-anchor-banner__icon {
				font-size: 14px;
				color: var(--rose, #b8849e);
				flex-shrink: 0;
				margin-top: 2px;
			}
			.gf-anchor-banner__text {
				font-size: 11px;
				color: var(--text-2, #adb5be);
				line-height: 1.6;
			}
			.gf-anchor-banner__text strong {
				color: var(--rose, #b8849e);
			}

			.gf-field {
				display: flex;
				flex-direction: column;
				gap: 6px;
			}
			.gf-label {
				font-size: 11px;
				font-weight: 600;
				color: var(--text-2, #adb5be);
			}
			.gf-textarea {
				padding: 10px 12px;
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 4px;
				background: var(--surface-2, #1c232d);
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				resize: vertical;
				min-height: 120px;
				line-height: 1.6;
			}
			.gf-textarea:focus {
				outline: none;
				border-color: var(--rose-border, rgba(184, 132, 158, 0.3));
			}
			.gf-hint {
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				line-height: 1.5;
			}

			.gf-assist-row {
				display: flex;
				align-items: center;
				gap: 10px;
			}
			.gf-btn--assist {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 6px 14px;
				border: 1px solid var(--rose-border, rgba(184, 132, 158, 0.25));
				border-radius: 4px;
				background: var(--rose-dim, rgba(184, 132, 158, 0.08));
				font-family: var(--font-mono, monospace);
				font-size: 11px;
				font-weight: 600;
				color: var(--rose, #b8849e);
				cursor: pointer;
			}
			.gf-btn--assist:hover {
				background: color-mix(
					in srgb,
					var(--rose-dim, rgba(184, 132, 158, 0.08)) 50%,
					var(--rose, #b8849e) 10%
				);
			}

			.gf-footer {
				display: flex;
				align-items: center;
				gap: 10px;
				padding: 14px 20px;
				border-top: 1px solid var(--border-color, #30363d);
			}
			.gf-btn--back {
				padding: 6px 14px;
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 4px;
				background: transparent;
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				color: var(--text-2, #adb5be);
				cursor: pointer;
			}
			.gf-btn--next {
				padding: 6px 14px;
				border: 1px solid var(--rose-border, rgba(184, 132, 158, 0.25));
				border-radius: 4px;
				background: var(--rose-dim, rgba(184, 132, 158, 0.08));
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				font-weight: 600;
				color: var(--rose, #b8849e);
				cursor: pointer;
			}
		</style>

		<div class="gf-shell">
			<header class="gf-header">
				<button type="button" class="gf-back" aria-label="Go back">←</button>
				<span class="gf-title">New steerer</span>
				<span class="gf-subtitle">· guided</span>
			</header>

			<nav class="gf-steps" aria-label="Create flow steps">
				<div class="gf-step gf-step--done"><span class="gf-step__num">✓</span> Session</div>
				<span class="gf-step-sep">›</span>
				<div class="gf-step gf-step--done"><span class="gf-step__num">✓</span> Overview</div>
				<span class="gf-step-sep">›</span>
				<div class="gf-step gf-step--active"><span class="gf-step__num">3</span> Define steer</div>
				<span class="gf-step-sep">›</span>
				<div class="gf-step gf-step--idle"><span class="gf-step__num">4</span> Confirm</div>
			</nav>

			<div class="gf-body">
				<div>
					<h2 class="gf-step-heading">What should the steerer watch for?</h2>
					<p class="gf-step-subtext">
						Describe in plain language what this session should be focused on. The steerer uses this
						as its behavioral anchor — the constraint it evaluates each new turn against.
					</p>
				</div>

				<div class="gf-anchor-banner" role="note">
					<span class="gf-anchor-banner__icon" aria-hidden="true">⊕</span>
					<div class="gf-anchor-banner__text">
						Targeting <strong>response #42</strong> as the anchor point. The intent field below is pre-filled
						with diagnostic findings from that response. Edit or replace the text before clicking "Help
						me write this."
					</div>
				</div>

				<div class="gf-field">
					<label class="gf-label" for="gf-intent">What should this session be focused on?</label>
					<textarea id="gf-intent" class="gf-textarea" rows="7">{MOCK_PREFILL_INTENT}</textarea>
					<p class="gf-hint">
						This will become the steerer's behavioral constraint — the rule the runtime steerer
						evaluates each turn against. Be specific about what behaviors to forbid.
					</p>
				</div>

				<div class="gf-assist-row">
					<button type="button" class="gf-btn--assist">
						<span aria-hidden="true">✦</span>
						Help me write this →
					</button>
					<span style="font-size: 11px; color: var(--text-3, #c0c7d0);">
						Uses the steer-author agent to turn your description into an effective anchor.
					</span>
				</div>
			</div>

			<footer class="gf-footer">
				<button type="button" class="gf-btn--back">← Back</button>
				<button type="button" class="gf-btn--next">Review + create →</button>
			</footer>
		</div>
	</div>
</Story>

<Story
	name="Step3_AssistThinking"
	parameters={{
		docs: {
			description: {
				story:
					'"Help me write this" clicked — steer-author agent call in flight. Textarea remains editable. Button replaced by spinner. Per spec §12.9.3: no skeleton rows, no panel collapse — rest of Step 3 is unchanged while waiting.'
			}
		}
	}}
>
	<div class="sb-wrapper">
		<style>
			.sb-wrapper {
				background: var(--bg, #0f1117);
				min-height: 100vh;
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-mono, monospace);
				font-size: 13px;
			}
			.gf-shell {
				display: flex;
				flex-direction: column;
				height: 100vh;
			}
			.gf-header {
				display: flex;
				align-items: center;
				gap: 12px;
				padding: 14px 20px;
				border-bottom: 1px solid var(--border-color, #30363d);
			}
			.gf-back {
				display: inline-flex;
				align-items: center;
				justify-content: center;
				width: 28px;
				height: 28px;
				border-radius: 4px;
				border: 1px solid var(--border-subtle, #23282f);
				color: var(--text-2, #adb5be);
				cursor: pointer;
				background: transparent;
				font-size: 14px;
			}
			.gf-title {
				font-size: 15px;
				font-weight: 700;
				color: var(--text-strong, #dde2e8);
			}
			.gf-steps {
				display: flex;
				align-items: center;
				gap: 0;
				padding: 0 20px;
				border-bottom: 1px solid var(--border-subtle, #23282f);
				height: 40px;
			}
			.gf-step {
				display: flex;
				align-items: center;
				gap: 6px;
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				padding: 0 12px 0 0;
			}
			.gf-step--done {
				color: var(--green, #4a9f6f);
			}
			.gf-step--active {
				color: var(--rose, #b8849e);
				font-weight: 600;
			}
			.gf-step--idle {
				color: var(--text-3, #c0c7d0);
			}
			.gf-step__num {
				display: inline-flex;
				align-items: center;
				justify-content: center;
				width: 18px;
				height: 18px;
				border-radius: 50%;
				border: 1px solid;
				font-size: 10px;
				font-weight: 700;
			}
			.gf-step--done .gf-step__num {
				border-color: var(--green, #4a9f6f);
				color: var(--green, #4a9f6f);
			}
			.gf-step--active .gf-step__num {
				border-color: var(--rose, #b8849e);
				background: var(--rose-dim, rgba(184, 132, 158, 0.08));
				color: var(--rose, #b8849e);
			}
			.gf-step--idle .gf-step__num {
				border-color: var(--border-subtle, #23282f);
			}
			.gf-step-sep {
				font-size: 10px;
				color: var(--text-faint, #c0c7d0);
				margin-right: 12px;
			}
			.gf-body {
				flex: 1;
				overflow-y: auto;
				padding: 24px 20px;
				display: flex;
				flex-direction: column;
				gap: 20px;
				max-width: 640px;
			}
			.gf-step-heading {
				font-size: 16px;
				font-weight: 700;
				color: var(--text-strong, #dde2e8);
				margin: 0;
			}
			.gf-field {
				display: flex;
				flex-direction: column;
				gap: 6px;
			}
			.gf-label {
				font-size: 11px;
				font-weight: 600;
				color: var(--text-2, #adb5be);
			}
			.gf-textarea {
				padding: 10px 12px;
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 4px;
				background: var(--surface-2, #1c232d);
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				resize: vertical;
				min-height: 120px;
				line-height: 1.6;
			}
			.gf-hint {
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
			}

			.gf-btn--thinking {
				display: inline-flex;
				align-items: center;
				gap: 8px;
				padding: 6px 14px;
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 4px;
				background: var(--surface-2, #1c232d);
				font-family: var(--font-mono, monospace);
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				cursor: not-allowed;
				opacity: 0.75;
			}

			.spin {
				width: 12px;
				height: 12px;
				border: 2px solid var(--border-subtle, #23282f);
				border-top-color: var(--rose, #b8849e);
				border-radius: 50%;
				animation: spin 0.7s linear infinite;
				flex-shrink: 0;
			}
			@keyframes spin {
				to {
					transform: rotate(360deg);
				}
			}

			.gf-footer {
				display: flex;
				align-items: center;
				gap: 10px;
				padding: 14px 20px;
				border-top: 1px solid var(--border-color, #30363d);
			}
			.gf-btn--back {
				padding: 6px 14px;
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 4px;
				background: transparent;
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				color: var(--text-2, #adb5be);
				cursor: pointer;
			}
			.gf-btn--next {
				padding: 6px 14px;
				border: 1px solid var(--rose-border, rgba(184, 132, 158, 0.25));
				border-radius: 4px;
				background: var(--rose-dim, rgba(184, 132, 158, 0.08));
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				font-weight: 600;
				color: var(--rose, #b8849e);
				cursor: pointer;
			}
		</style>

		<div class="gf-shell">
			<header class="gf-header">
				<button type="button" class="gf-back">←</button>
				<span class="gf-title">New steerer</span>
			</header>
			<nav class="gf-steps">
				<div class="gf-step gf-step--done"><span class="gf-step__num">✓</span> Session</div>
				<span class="gf-step-sep">›</span>
				<div class="gf-step gf-step--done"><span class="gf-step__num">✓</span> Overview</div>
				<span class="gf-step-sep">›</span>
				<div class="gf-step gf-step--active"><span class="gf-step__num">3</span> Define steer</div>
				<span class="gf-step-sep">›</span>
				<div class="gf-step gf-step--idle"><span class="gf-step__num">4</span> Confirm</div>
			</nav>
			<div class="gf-body">
				<h2 class="gf-step-heading">What should the steerer watch for?</h2>
				<div class="gf-field">
					<label class="gf-label" for="gf-intent-think">Intent</label>
					<textarea id="gf-intent-think" class="gf-textarea" rows="7"
						>{MOCK_PREFILL_INTENT}</textarea
					>
					<p class="gf-hint">Textarea remains editable while the steer-author agent works.</p>
				</div>
				<div>
					<button type="button" class="gf-btn--thinking" disabled aria-busy="true">
						<span class="spin" aria-hidden="true"></span>
						Drafting suggestion…
					</button>
				</div>
			</div>
			<footer class="gf-footer">
				<button type="button" class="gf-btn--back">← Back</button>
				<button type="button" class="gf-btn--next">Review + create →</button>
			</footer>
		</div>
	</div>
</Story>

<Story
	name="Step3_SuggestionPanel"
	parameters={{
		docs: {
			description: {
				story:
					'Steer-author agent has returned. Shows the authored anchor (a behavioral constraint, not a reply) with per-field rationale and model provenance. Full content — no truncation. Accept pre-fills all fields; Edit manually / Dismiss leave them unchanged.'
			}
		}
	}}
>
	<div class="sb-wrapper">
		<style>
			.sb-wrapper {
				background: var(--bg, #0f1117);
				min-height: 100vh;
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-mono, monospace);
				font-size: 13px;
			}
			.gf-shell {
				display: flex;
				flex-direction: column;
				height: 100vh;
			}
			.gf-header {
				display: flex;
				align-items: center;
				gap: 12px;
				padding: 14px 20px;
				border-bottom: 1px solid var(--border-color, #30363d);
			}
			.gf-back {
				display: inline-flex;
				align-items: center;
				justify-content: center;
				width: 28px;
				height: 28px;
				border-radius: 4px;
				border: 1px solid var(--border-subtle, #23282f);
				color: var(--text-2, #adb5be);
				cursor: pointer;
				background: transparent;
				font-size: 14px;
			}
			.gf-title {
				font-size: 15px;
				font-weight: 700;
				color: var(--text-strong, #dde2e8);
			}
			.gf-steps {
				display: flex;
				align-items: center;
				gap: 0;
				padding: 0 20px;
				border-bottom: 1px solid var(--border-subtle, #23282f);
				height: 40px;
			}
			.gf-step {
				display: flex;
				align-items: center;
				gap: 6px;
				font-size: 11px;
				padding: 0 12px 0 0;
			}
			.gf-step--done {
				color: var(--green, #4a9f6f);
			}
			.gf-step--active {
				color: var(--rose, #b8849e);
				font-weight: 600;
			}
			.gf-step--idle {
				color: var(--text-3, #c0c7d0);
			}
			.gf-step__num {
				display: inline-flex;
				align-items: center;
				justify-content: center;
				width: 18px;
				height: 18px;
				border-radius: 50%;
				border: 1px solid;
				font-size: 10px;
				font-weight: 700;
			}
			.gf-step--done .gf-step__num {
				border-color: var(--green, #4a9f6f);
				color: var(--green, #4a9f6f);
			}
			.gf-step--active .gf-step__num {
				border-color: var(--rose, #b8849e);
				background: var(--rose-dim, rgba(184, 132, 158, 0.08));
				color: var(--rose, #b8849e);
			}
			.gf-step--idle .gf-step__num {
				border-color: var(--border-subtle, #23282f);
			}
			.gf-step-sep {
				font-size: 10px;
				color: var(--text-faint, #c0c7d0);
				margin-right: 12px;
			}
			.gf-body {
				flex: 1;
				overflow-y: auto;
				padding: 24px 20px;
				display: flex;
				flex-direction: column;
				gap: 16px;
				max-width: 640px;
			}
			.gf-step-heading {
				font-size: 16px;
				font-weight: 700;
				color: var(--text-strong, #dde2e8);
				margin: 0;
			}

			.gf-field {
				display: flex;
				flex-direction: column;
				gap: 6px;
			}
			.gf-label {
				font-size: 11px;
				font-weight: 600;
				color: var(--text-2, #adb5be);
			}
			.gf-textarea {
				padding: 10px 12px;
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 4px;
				background: var(--surface-2, #1c232d);
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				resize: vertical;
				min-height: 80px;
				line-height: 1.6;
				opacity: 0.65;
			}

			.gf-suggest {
				border: 1px solid var(--rose-border, rgba(184, 132, 158, 0.25));
				border-radius: 6px;
				background: color-mix(
					in srgb,
					var(--rose-dim, rgba(184, 132, 158, 0.08)) 80%,
					var(--surface, #161b22) 20%
				);
				display: flex;
				flex-direction: column;
				gap: 0;
				overflow: hidden;
			}

			.gf-suggest-header {
				display: flex;
				align-items: center;
				justify-content: space-between;
				padding: 10px 14px;
				border-bottom: 1px solid var(--rose-border, rgba(184, 132, 158, 0.15));
			}

			.gf-suggest-title {
				font-size: 11px;
				font-weight: 600;
				color: var(--rose, #b8849e);
				display: flex;
				align-items: center;
				gap: 6px;
			}

			.gf-suggest-provenance {
				font-size: 10px;
				color: var(--text-3, #c0c7d0);
				font-weight: 400;
			}

			.gf-suggest-dismiss {
				width: 20px;
				height: 20px;
				display: inline-flex;
				align-items: center;
				justify-content: center;
				border: none;
				background: transparent;
				color: var(--text-3, #c0c7d0);
				cursor: pointer;
				border-radius: 3px;
				font-size: 14px;
				line-height: 1;
				flex-shrink: 0;
			}
			.gf-suggest-dismiss:hover {
				color: var(--text-1, #c2c9d1);
				background: var(--surface-2, #1c232d);
			}

			.gf-suggest-body {
				display: flex;
				flex-direction: column;
				gap: 0;
				padding: 0;
				max-height: 360px;
				overflow-y: auto;
			}

			.gf-sug-row {
				display: flex;
				flex-direction: column;
				gap: 4px;
				padding: 12px 14px;
				border-bottom: 1px solid var(--rose-border, rgba(184, 132, 158, 0.1));
			}
			.gf-sug-row:last-child {
				border-bottom: none;
			}

			.gf-sug-field-label {
				font-size: 10px;
				font-weight: 600;
				text-transform: uppercase;
				letter-spacing: 0.06em;
				color: var(--text-3, #c0c7d0);
			}

			.gf-sug-anchor-text {
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				line-height: 1.65;
				color: var(--text-strong, #dde2e8);
				white-space: pre-wrap;
				word-break: break-word;
				padding: 10px 12px;
				background: var(--surface-2, #1c232d);
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 4px;
			}

			.gf-sug-value {
				font-size: 12px;
				color: var(--text-strong, #dde2e8);
				font-weight: 500;
			}

			.gf-sug-rationale {
				font-size: 11px;
				color: var(--text-2, #adb5be);
				line-height: 1.55;
				display: flex;
				align-items: flex-start;
				gap: 6px;
			}

			.gf-sug-rationale::before {
				content: '↳';
				color: var(--text-faint, #c0c7d0);
				font-size: 11px;
				flex-shrink: 0;
				margin-top: 2px;
			}

			.gf-suggest-actions {
				display: flex;
				align-items: center;
				gap: 10px;
				padding: 12px 14px;
				border-top: 1px solid var(--rose-border, rgba(184, 132, 158, 0.15));
			}

			.gf-btn--accept {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 6px 14px;
				border: 1px solid var(--rose-border, rgba(184, 132, 158, 0.3));
				border-radius: 4px;
				background: var(--rose-dim, rgba(184, 132, 158, 0.12));
				font-family: var(--font-mono, monospace);
				font-size: 11px;
				font-weight: 600;
				color: var(--rose, #b8849e);
				cursor: pointer;
			}
			.gf-btn--accept:hover {
				background: color-mix(
					in srgb,
					var(--rose-dim, rgba(184, 132, 158, 0.12)) 50%,
					var(--rose, #b8849e) 15%
				);
			}

			.gf-btn--edit-manual {
				padding: 6px 14px;
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 4px;
				background: transparent;
				font-family: var(--font-mono, monospace);
				font-size: 11px;
				color: var(--text-2, #adb5be);
				cursor: pointer;
			}

			.gf-footer {
				display: flex;
				align-items: center;
				gap: 10px;
				padding: 14px 20px;
				border-top: 1px solid var(--border-color, #30363d);
			}
			.gf-btn--back {
				padding: 6px 14px;
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 4px;
				background: transparent;
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				color: var(--text-2, #adb5be);
				cursor: pointer;
			}
			.gf-btn--next {
				padding: 6px 14px;
				border: 1px solid var(--rose-border, rgba(184, 132, 158, 0.25));
				border-radius: 4px;
				background: var(--rose-dim, rgba(184, 132, 158, 0.08));
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				font-weight: 600;
				color: var(--rose, #b8849e);
				cursor: pointer;
			}
		</style>

		<div class="gf-shell">
			<header class="gf-header">
				<button type="button" class="gf-back">←</button>
				<span class="gf-title">New steerer</span>
			</header>
			<nav class="gf-steps">
				<div class="gf-step gf-step--done"><span class="gf-step__num">✓</span> Session</div>
				<span class="gf-step-sep">›</span>
				<div class="gf-step gf-step--done"><span class="gf-step__num">✓</span> Overview</div>
				<span class="gf-step-sep">›</span>
				<div class="gf-step gf-step--active"><span class="gf-step__num">3</span> Define steer</div>
				<span class="gf-step-sep">›</span>
				<div class="gf-step gf-step--idle"><span class="gf-step__num">4</span> Confirm</div>
			</nav>

			<div class="gf-body">
				<h2 class="gf-step-heading">What should the steerer watch for?</h2>

				<div class="gf-field">
					<label class="gf-label" for="gf-intent-sug">Intent (editable)</label>
					<textarea id="gf-intent-sug" class="gf-textarea" rows="4">{MOCK_PREFILL_INTENT}</textarea>
				</div>

				<div class="gf-suggest" role="region" aria-label="Steer-author suggestion">
					<div class="gf-suggest-header">
						<div class="gf-suggest-title">
							<span aria-hidden="true">✦</span>
							Suggestion
							<span class="gf-suggest-provenance">from openai/gpt-5.6-luna</span>
						</div>
						<button
							type="button"
							class="gf-suggest-dismiss"
							title="Dismiss suggestion (Edit manually)"
							aria-label="Dismiss suggestion">✕</button
						>
					</div>

					<div class="gf-suggest-body">
						<div class="gf-sug-row">
							<span class="gf-sug-field-label">Steering anchor (behavioral constraint)</span>
							<pre class="gf-sug-anchor-text">{MOCK_AUTHORED_ANCHOR}</pre>
							<p class="gf-sug-rationale">{MOCK_RATIONALE.anchor}</p>
						</div>

						<div class="gf-sug-row">
							<span class="gf-sug-field-label">Authority</span>
							<span class="gf-sug-value">supply_context</span>
							<p class="gf-sug-rationale">{MOCK_RATIONALE.authority}</p>
						</div>

						<div class="gf-sug-row">
							<span class="gf-sug-field-label">Mode</span>
							<span class="gf-sug-value">augment</span>
							<p class="gf-sug-rationale">{MOCK_RATIONALE.mode}</p>
						</div>

						<div class="gf-sug-row">
							<span class="gf-sug-field-label">Confidence threshold</span>
							<span class="gf-sug-value">0.80</span>
							<p class="gf-sug-rationale">{MOCK_RATIONALE.confidence_threshold}</p>
						</div>

						<div class="gf-sug-row">
							<span class="gf-sug-field-label">Debounce turns</span>
							<span class="gf-sug-value">1 turn</span>
							<p class="gf-sug-rationale">{MOCK_RATIONALE.debounce_turns}</p>
						</div>
					</div>

					<div class="gf-suggest-actions">
						<button type="button" class="gf-btn--accept">Accept suggestion</button>
						<button type="button" class="gf-btn--edit-manual">Edit manually</button>
					</div>
				</div>
			</div>

			<footer class="gf-footer">
				<button type="button" class="gf-btn--back">← Back</button>
				<button type="button" class="gf-btn--next">Review + create →</button>
			</footer>
		</div>
	</div>
</Story>

<Story
	name="AnchorQuality_BadVsGood"
	parameters={{
		docs: {
			description: {
				story:
					'Side-by-side: current generic-completion output (bug — reads like a reply) vs the steer-author agent output (correct — behavioral constraint). Source: weasel-steer-dryrun-spec.md §3.2 (bug) + §3.4 (fix).'
			}
		}
	}}
>
	<div class="sb-wrapper">
		<style>
			.sb-wrapper {
				background: var(--bg, #0f1117);
				min-height: 100vh;
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-mono, monospace);
				font-size: 13px;
				padding: 28px;
			}
			.compare-heading {
				font-size: 14px;
				font-weight: 700;
				color: var(--text-strong, #dde2e8);
				margin-bottom: 4px;
			}
			.compare-subtext {
				font-size: 11px;
				color: var(--text-2, #adb5be);
				margin-bottom: 20px;
				line-height: 1.5;
			}
			.compare-grid {
				display: grid;
				grid-template-columns: 1fr 1fr;
				gap: 20px;
			}
			.compare-col-head {
				font-size: 10px;
				font-weight: 700;
				text-transform: uppercase;
				letter-spacing: 0.07em;
				margin-bottom: 10px;
			}
			.compare-col-head--wrong {
				color: var(--red, #c48080);
			}
			.compare-col-head--right {
				color: var(--green, #4a9f6f);
			}
			.compare-box {
				padding: 14px 16px;
				border-radius: 6px;
				font-size: 12px;
				line-height: 1.65;
				white-space: pre-wrap;
				word-break: break-word;
			}
			.compare-box--wrong {
				border: 1px solid rgba(196, 128, 128, 0.25);
				background: rgba(196, 128, 128, 0.05);
				color: var(--text-2, #adb5be);
			}
			.compare-box--right {
				border: 1px solid var(--rose-border, rgba(184, 132, 158, 0.25));
				background: var(--rose-dim, rgba(184, 132, 158, 0.08));
				color: var(--text-1, #c2c9d1);
			}
			.compare-caption {
				margin-top: 10px;
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				line-height: 1.5;
			}
			.compare-caption strong {
				display: block;
				color: var(--text-2, #adb5be);
				margin-bottom: 4px;
			}
			.rule-note {
				margin-top: 24px;
				padding: 12px 14px;
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 6px;
				background: var(--surface-2, #1c232d);
				font-size: 11px;
				color: var(--text-2, #adb5be);
				line-height: 1.65;
			}
			.rule-note strong {
				color: var(--text-strong, #dde2e8);
			}
		</style>

		<p class="compare-heading">Steering anchor quality: bug vs correct design</p>
		<p class="compare-subtext">
			The current <code>POST /api/steerers/assist</code> uses a generic completion prompt that primes
			the LLM to describe what the session is doing. The steer-author agent uses a tuned system prompt
			(spec §3.4) that produces a behavioral constraint — a directive the runtime steerer enforces.
		</p>

		<div class="compare-grid">
			<div>
				<p class="compare-col-head compare-col-head--wrong">✗ Current (generic completion — BUG)</p>
				<div class="compare-box compare-box--wrong">
					The session is currently working on implementing the steering fleet backend in <code
						>crates/mike-features/src/steerer/</code
					>. The assistant has been focused on writing Rust code for the fleet evaluation loop. You
					are right that a RIGHT JOIN would be better for this query. The session should continue
					working on Phase 13.
				</div>
				<p class="compare-caption">
					<strong>Why this is wrong</strong>
					This reads like a continuation of the conversation. The runtime steerer at
					<code>refocus.rs:23-29</code> uses this as the system prompt for drift detection. Feeding it
					a session description or a reply means the LLM cannot answer "does this turn violate the constraint?"
					— because there is no constraint.
				</p>
			</div>
			<div>
				<p class="compare-col-head compare-col-head--right">
					✓ Steer-author agent (spec §3.4 — correct anchor shape)
				</p>
				<div class="compare-box compare-box--right">{MOCK_AUTHORED_ANCHOR}</div>
				<p class="compare-caption">
					<strong>Why this is correct</strong>
					A directive in the imperative. Names the forbidden behaviors explicitly. Reads as a rule the
					steerer evaluates a turn against, not a reply to the session. The runtime steerer can now answer
					"does this turn violate the constraint?" reliably.
				</p>
			</div>
		</div>

		<div class="rule-note">
			<strong>Rule (spec §3.4)</strong>
			The authored <code>suggestion.prompt</code> must be a directive of what to hold the session to
			and what behaviors to forbid — NOT a continuation of the conversation. The steer-author
			agent's system prompt is the quality lever. See
			<code>artifacts/2026-06-16/weasel-steer-dryrun-spec.md §3.4</code> for the full tuned prompt.
		</div>
	</div>
</Story>
