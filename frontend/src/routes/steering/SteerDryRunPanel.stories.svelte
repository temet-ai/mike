<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';

	const { Story } = defineMeta({
		title: 'Routes/Steering/SteerDryRunPanel',
		tags: ['autodocs'],
		parameters: {
			layout: 'fullscreen',
			docs: {
				description: {
					component: `
Dry-run panel stories for Phase C (weasel-steer-BUILD-PLAN.md).

Surface 4 of 4: The dry-run panel renders INSIDE Step 3 of the guided flow,
below the anchor prompt input and the steer-author suggestion panel. It lets
the operator test a draft steer against the session's recent turns WITHOUT
enrolling — so they can judge firing behavior before committing.

API: POST /api/steerers/dry-run
Input: { session_id, anchor_request_id?, anchor_prompt, confidence_threshold }
Output: per-turn { would_fire, confidence, injection_preview, violation_topic, command }

Design spec: artifacts/2026-06-16/weasel-steer-dryrun-spec.md §4
Build plan: artifacts/2026-06-16/weasel-steer-BUILD-PLAN.md Phase C item 4

Inspection fidelity rule: injection_preview renders in FULL — pre-wrap, scrollable
container, no line-clamp, no truncation. The preview IS the diagnostic artifact.
          `.trim()
				}
			}
		}
	});
</script>

<Story name="DryRun_ButtonIdle">
	<div class="mock-step3-shell">
		<style>
			.mock-step3-shell {
				background: var(--bg, #0f1117);
				min-height: 100vh;
				padding: 32px;
				font-family: var(--font-mono, monospace);
				color: var(--text-1, #c2c9d1);
			}

			.gf-card {
				background: var(--surface, #161b22);
				border: 1px solid var(--border, rgba(194, 201, 209, 0.1));
				border-radius: 6px;
				max-width: 780px;
				margin: 0 auto;
				overflow: hidden;
			}
			.gf-card-header {
				padding: 14px 20px;
				border-bottom: 1px solid var(--border, rgba(194, 201, 209, 0.1));
				display: flex;
				align-items: center;
				gap: 10px;
			}
			.gf-step-badge {
				font-size: 10px;
				font-weight: 700;
				color: var(--rose, #b8849e);
				background: var(--rose-dim, rgba(184, 132, 158, 0.08));
				border: 1px solid var(--rose-border, rgba(184, 132, 158, 0.15));
				border-radius: 3px;
				padding: 2px 7px;
				letter-spacing: 0.06em;
			}
			.gf-card-title {
				font-size: 12px;
				font-weight: 600;
				color: var(--text-1, #c2c9d1);
			}
			.gf-card-body {
				padding: 20px;
				display: flex;
				flex-direction: column;
				gap: 16px;
			}

			.anchor-summary {
				background: var(--surface-2, #1c232d);
				border: 1px solid var(--rose-border, rgba(184, 132, 158, 0.15));
				border-radius: 4px;
				padding: 12px 14px;
			}
			.anchor-summary-label {
				font-size: 10px;
				font-weight: 600;
				color: var(--rose, #b8849e);
				letter-spacing: 0.08em;
				margin-bottom: 6px;
				text-transform: uppercase;
			}
			.anchor-summary-text {
				font-size: 11px;
				color: var(--text-2, #adb5be);
				line-height: 1.6;
				white-space: pre-wrap;
			}
			.anchor-summary-meta {
				margin-top: 8px;
				font-size: 10px;
				color: var(--text-3, #c0c7d0);
				display: flex;
				gap: 14px;
			}
			.anchor-meta-chip {
				background: var(--surface, #161b22);
				border: 1px solid var(--border, rgba(194, 201, 209, 0.1));
				border-radius: 3px;
				padding: 1px 6px;
			}

			.dry-run-action-row {
				display: flex;
				align-items: center;
				gap: 10px;
				padding-top: 4px;
			}
			.dry-run-btn {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 6px 14px;
				border: 1px solid var(--rose-border, rgba(184, 132, 158, 0.15));
				border-radius: 4px;
				background: var(--rose-dim, rgba(184, 132, 158, 0.08));
				font-family: var(--font-mono, monospace);
				font-size: 11px;
				font-weight: 600;
				color: var(--rose, #b8849e);
				cursor: pointer;
				transition:
					background 0.15s,
					border-color 0.15s;
			}
			.dry-run-btn:hover {
				background: rgba(184, 132, 158, 0.14);
				border-color: rgba(184, 132, 158, 0.28);
			}
			.dry-run-btn-icon {
				font-size: 13px;
			}
			.dry-run-hint {
				font-size: 10px;
				color: var(--text-3, #c0c7d0);
			}

			.gf-footer-row {
				display: flex;
				align-items: center;
				justify-content: space-between;
				padding-top: 4px;
				border-top: 1px solid var(--border, rgba(194, 201, 209, 0.08));
			}
			.gf-btn-enrol {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 7px 16px;
				border: 1px solid var(--rose-border, rgba(184, 132, 158, 0.15));
				border-radius: 4px;
				background: var(--rose-dim, rgba(184, 132, 158, 0.08));
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				font-weight: 600;
				color: var(--rose, #b8849e);
				cursor: pointer;
				transition: background 0.15s;
			}
			.gf-btn-secondary {
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				background: none;
				border: none;
				cursor: pointer;
				font-family: var(--font-mono, monospace);
				padding: 4px 8px;
			}

			.annotation {
				margin-top: 24px;
				border: 1px solid var(--border, rgba(194, 201, 209, 0.1));
				border-radius: 4px;
				padding: 12px 16px;
				font-size: 11px;
				color: var(--text-2, #adb5be);
				background: var(--surface, #161b22);
				max-width: 780px;
				margin-left: auto;
				margin-right: auto;
			}
			.annotation strong {
				color: var(--text-1, #c2c9d1);
			}
		</style>

		<div class="gf-card">
			<div class="gf-card-header">
				<span class="gf-step-badge">STEP 3</span>
				<span class="gf-card-title">Craft the steer</span>
			</div>
			<div class="gf-card-body">
				<div class="anchor-summary">
					<div class="anchor-summary-label">Anchor</div>
					<div class="anchor-summary-text">
						Hold this session to the originally requested deliverable: fixing the failing migration
						test in crates/mike/tests/integration/persistence.rs. The session MUST NOT: - Claim that
						a test failure is pre-existing or out-of-scope without evidence from git blame or a
						bisect. - Claim "all tests pass" or "tests are passing" without showing actual nextest
						output. - Declare a deliverable complete without a working, verified result.
					</div>
					<div class="anchor-summary-meta">
						<span class="anchor-meta-chip">authority: system</span>
						<span class="anchor-meta-chip">mode: redirect-attention</span>
						<span class="anchor-meta-chip">confidence ≥ 0.75</span>
						<span class="anchor-meta-chip">debounce: 2 turns</span>
					</div>
				</div>

				<div class="dry-run-action-row">
					<button class="dry-run-btn" type="button">
						<span class="dry-run-btn-icon">⟳</span>
						Test this steer →
					</button>
					<span class="dry-run-hint"
						>Runs the steer against this session's recent turns without enrolling</span
					>
				</div>

				<div class="gf-footer-row">
					<button class="gf-btn-secondary" type="button">← Back</button>
					<button class="gf-btn-enrol" type="button">Enrol →</button>
				</div>
			</div>
		</div>

		<div class="annotation">
			<strong>Design note:</strong> "Test this steer →" appears BELOW the anchor area, ABOVE the
			enrol button. It is always present once an anchor exists (authored or manually written). The
			operator can skip it and enrol directly, or test first to build confidence. Clicking it POSTs
			to
			<code>POST /api/steerers/dry-run</code> with the current draft anchor text + session context.
		</div>
	</div>
</Story>

<Story name="DryRun_Loading">
	<div class="mock-step3-shell">
		<style>
			.mock-step3-shell {
				background: var(--bg, #0f1117);
				min-height: 100vh;
				padding: 32px;
				font-family: var(--font-mono, monospace);
				color: var(--text-1, #c2c9d1);
			}
			.gf-card {
				background: var(--surface, #161b22);
				border: 1px solid var(--border, rgba(194, 201, 209, 0.1));
				border-radius: 6px;
				max-width: 780px;
				margin: 0 auto;
				overflow: hidden;
			}
			.gf-card-header {
				padding: 14px 20px;
				border-bottom: 1px solid var(--border, rgba(194, 201, 209, 0.1));
				display: flex;
				align-items: center;
				gap: 10px;
			}
			.gf-step-badge {
				font-size: 10px;
				font-weight: 700;
				color: var(--rose, #b8849e);
				background: var(--rose-dim, rgba(184, 132, 158, 0.08));
				border: 1px solid var(--rose-border, rgba(184, 132, 158, 0.15));
				border-radius: 3px;
				padding: 2px 7px;
				letter-spacing: 0.06em;
			}
			.gf-card-title {
				font-size: 12px;
				font-weight: 600;
				color: var(--text-1, #c2c9d1);
			}
			.gf-card-body {
				padding: 20px;
				display: flex;
				flex-direction: column;
				gap: 16px;
			}

			.dry-run-panel {
				border: 1px solid var(--rose-border, rgba(184, 132, 158, 0.15));
				border-radius: 4px;
				overflow: hidden;
			}
			.dry-run-panel-header {
				background: var(--rose-dim, rgba(184, 132, 158, 0.08));
				padding: 10px 14px;
				display: flex;
				align-items: center;
				gap: 10px;
				font-size: 11px;
				font-weight: 600;
				color: var(--rose, #b8849e);
			}
			.dry-run-spinner {
				width: 14px;
				height: 14px;
				border: 2px solid var(--rose-border, rgba(184, 132, 158, 0.3));
				border-top-color: var(--rose, #b8849e);
				border-radius: 50%;
				animation: dry-run-spin 0.7s linear infinite;
				flex-shrink: 0;
			}
			@keyframes dry-run-spin {
				to {
					transform: rotate(360deg);
				}
			}
			.dry-run-panel-body {
				background: var(--surface-2, #1c232d);
				padding: 12px 14px;
				display: flex;
				flex-direction: column;
				gap: 8px;
			}

			.dry-run-skeleton-row {
				display: flex;
				align-items: center;
				gap: 10px;
				padding: 8px 0;
				border-bottom: 1px solid var(--border, rgba(194, 201, 209, 0.06));
			}
			.dry-run-skeleton-row:last-child {
				border-bottom: none;
			}
			.skeleton-dot {
				width: 10px;
				height: 10px;
				border-radius: 50%;
				background: var(--border, rgba(194, 201, 209, 0.15));
				flex-shrink: 0;
				animation: skeleton-pulse 1.2s ease-in-out infinite;
			}
			.skeleton-line {
				height: 10px;
				border-radius: 3px;
				background: var(--border, rgba(194, 201, 209, 0.1));
				animation: skeleton-pulse 1.2s ease-in-out infinite;
			}
			@keyframes skeleton-pulse {
				0%,
				100% {
					opacity: 0.4;
				}
				50% {
					opacity: 0.9;
				}
			}

			.dry-run-turn-row {
				display: flex;
				align-items: flex-start;
				gap: 10px;
				padding: 8px 0;
				border-bottom: 1px solid var(--border, rgba(194, 201, 209, 0.06));
				font-size: 11px;
			}
			.dry-run-turn-row:last-child {
				border-bottom: none;
			}
			.turn-marker--pass {
				color: var(--text-3, #c0c7d0);
				font-size: 12px;
				flex-shrink: 0;
				margin-top: 1px;
			}
			.turn-marker--fire {
				color: var(--rose, #b8849e);
				font-size: 12px;
				flex-shrink: 0;
				margin-top: 1px;
			}
			.turn-label {
				color: var(--text-2, #adb5be);
				flex: 1;
				min-width: 0;
			}
			.turn-label strong {
				color: var(--text-1, #c2c9d1);
			}
			.turn-confidence {
				font-size: 10px;
				color: var(--text-3, #c0c7d0);
				white-space: nowrap;
			}

			.dry-run-btn--running {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 6px 14px;
				border: 1px solid var(--border, rgba(194, 201, 209, 0.1));
				border-radius: 4px;
				background: none;
				font-family: var(--font-mono, monospace);
				font-size: 11px;
				font-weight: 600;
				color: var(--text-3, #c0c7d0);
				cursor: not-allowed;
				opacity: 0.6;
			}
			.gf-footer-row {
				display: flex;
				align-items: center;
				justify-content: space-between;
				padding-top: 4px;
				border-top: 1px solid var(--border, rgba(194, 201, 209, 0.08));
			}
			.gf-btn-secondary {
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				background: none;
				border: none;
				cursor: pointer;
				font-family: var(--font-mono, monospace);
				padding: 4px 8px;
			}
			.gf-btn-enrol--disabled {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 7px 16px;
				border: 1px solid var(--border, rgba(194, 201, 209, 0.08));
				border-radius: 4px;
				background: none;
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				font-weight: 600;
				color: var(--text-3, #c0c7d0);
				cursor: not-allowed;
				opacity: 0.5;
			}

			.annotation {
				margin-top: 24px;
				border: 1px solid var(--border, rgba(194, 201, 209, 0.1));
				border-radius: 4px;
				padding: 12px 16px;
				font-size: 11px;
				color: var(--text-2, #adb5be);
				background: var(--surface, #161b22);
				max-width: 780px;
				margin-left: auto;
				margin-right: auto;
			}
			.annotation strong {
				color: var(--text-1, #c2c9d1);
			}
		</style>

		<div class="gf-card">
			<div class="gf-card-header">
				<span class="gf-step-badge">STEP 3</span>
				<span class="gf-card-title">Craft the steer</span>
			</div>
			<div class="gf-card-body">
				<div class="dry-run-panel">
					<div class="dry-run-panel-header">
						<div class="dry-run-spinner"></div>
						Testing against recent turns…
					</div>
					<div class="dry-run-panel-body">
						<div class="dry-run-turn-row">
							<span class="turn-marker--pass">✓</span>
							<span class="turn-label">
								<strong>Turn 1</strong> — user: "fix the failing migration test in persistence.rs"
							</span>
							<span class="turn-confidence">confidence: 0.08 — would NOT fire</span>
						</div>

						<div class="dry-run-skeleton-row">
							<div class="skeleton-dot"></div>
							<div class="skeleton-line" style="width: 60%; flex-shrink: 0;"></div>
							<div class="skeleton-line" style="width: 20%; margin-left: auto;"></div>
						</div>

						<div class="dry-run-skeleton-row">
							<div class="skeleton-dot" style="animation-delay: 0.2s;"></div>
							<div
								class="skeleton-line"
								style="width: 50%; flex-shrink: 0; animation-delay: 0.2s;"
							></div>
							<div
								class="skeleton-line"
								style="width: 22%; margin-left: auto; animation-delay: 0.2s;"
							></div>
						</div>

						<div class="dry-run-skeleton-row">
							<div class="skeleton-dot" style="animation-delay: 0.4s;"></div>
							<div
								class="skeleton-line"
								style="width: 55%; flex-shrink: 0; animation-delay: 0.4s;"
							></div>
							<div
								class="skeleton-line"
								style="width: 18%; margin-left: auto; animation-delay: 0.4s;"
							></div>
						</div>
					</div>
				</div>

				<div style="display: flex; align-items: center; gap: 10px;">
					<button class="dry-run-btn--running" type="button" disabled>
						<div class="dry-run-spinner" style="width: 11px; height: 11px;"></div>
						Testing…
					</button>
				</div>

				<div class="gf-footer-row">
					<button class="gf-btn-secondary" type="button">← Back</button>
					<button class="gf-btn-enrol--disabled" type="button" disabled>Enrol →</button>
				</div>
			</div>
		</div>

		<div class="annotation">
			<strong>Design note:</strong> Results arrive in parallel server-side. Completed turns render immediately;
			pending turns show skeleton rows. "Enrol →" is disabled while the dry-run is in progress (cursor:
			not-allowed, opacity: 0.5). The spinner in the panel header rotates continuously until all turns
			complete or a timeout fires.
		</div>
	</div>
</Story>

<Story name="DryRun_Results">
	<div class="mock-step3-shell">
		<style>
			.mock-step3-shell {
				background: var(--bg, #0f1117);
				min-height: 100vh;
				padding: 32px;
				font-family: var(--font-mono, monospace);
				color: var(--text-1, #c2c9d1);
			}
			.gf-card {
				background: var(--surface, #161b22);
				border: 1px solid var(--border, rgba(194, 201, 209, 0.1));
				border-radius: 6px;
				max-width: 780px;
				margin: 0 auto;
				overflow: hidden;
			}
			.gf-card-header {
				padding: 14px 20px;
				border-bottom: 1px solid var(--border, rgba(194, 201, 209, 0.1));
				display: flex;
				align-items: center;
				gap: 10px;
			}
			.gf-step-badge {
				font-size: 10px;
				font-weight: 700;
				color: var(--rose, #b8849e);
				background: var(--rose-dim, rgba(184, 132, 158, 0.08));
				border: 1px solid var(--rose-border, rgba(184, 132, 158, 0.15));
				border-radius: 3px;
				padding: 2px 7px;
				letter-spacing: 0.06em;
			}
			.gf-card-title {
				font-size: 12px;
				font-weight: 600;
				color: var(--text-1, #c2c9d1);
			}
			.gf-card-body {
				padding: 20px;
				display: flex;
				flex-direction: column;
				gap: 16px;
			}

			.dry-run-panel {
				border: 1px solid var(--rose-border, rgba(184, 132, 158, 0.15));
				border-radius: 4px;
				overflow: hidden;
			}
			.dry-run-panel-header {
				background: var(--rose-dim, rgba(184, 132, 158, 0.08));
				padding: 10px 14px;
				display: flex;
				align-items: center;
				justify-content: space-between;
				font-size: 11px;
			}
			.dry-run-panel-header-left {
				display: flex;
				align-items: center;
				gap: 8px;
				font-weight: 600;
				color: var(--rose, #b8849e);
			}
			.dry-run-fire-badge {
				background: var(--rose, #b8849e);
				color: #0f1117;
				font-size: 10px;
				font-weight: 700;
				padding: 1px 6px;
				border-radius: 3px;
			}
			.dry-run-panel-header-right {
				font-size: 10px;
				color: var(--text-3, #c0c7d0);
			}
			.dry-run-panel-body {
				background: var(--surface-2, #1c232d);
			}

			.dry-run-turn-row {
				padding: 10px 14px;
				border-bottom: 1px solid var(--border, rgba(194, 201, 209, 0.06));
				font-size: 11px;
			}
			.dry-run-turn-row:last-child {
				border-bottom: none;
			}

			.turn-row-header {
				display: flex;
				align-items: flex-start;
				gap: 10px;
			}
			.turn-marker--pass {
				color: var(--text-3, #c0c7d0);
				font-size: 13px;
				flex-shrink: 0;
				line-height: 1.4;
			}
			.turn-marker--fire {
				color: var(--rose, #b8849e);
				font-size: 13px;
				flex-shrink: 0;
				line-height: 1.4;
			}
			.turn-info {
				flex: 1;
				min-width: 0;
				display: flex;
				flex-direction: column;
				gap: 3px;
			}
			.turn-label {
				color: var(--text-2, #adb5be);
				line-height: 1.5;
			}
			.turn-label strong {
				color: var(--text-1, #c2c9d1);
			}
			.turn-result--pass {
				font-size: 10px;
				color: var(--text-3, #c0c7d0);
			}
			.turn-result--fire {
				font-size: 10px;
				color: var(--rose, #b8849e);
				font-weight: 600;
			}

			.turn-detail {
				margin-top: 10px;
				margin-left: 23px;
				display: flex;
				flex-direction: column;
				gap: 6px;
			}
			.turn-detail-field {
				display: flex;
				flex-direction: column;
				gap: 2px;
			}
			.turn-detail-key {
				font-size: 10px;
				color: var(--text-3, #c0c7d0);
				letter-spacing: 0.06em;
				text-transform: uppercase;
			}
			.turn-detail-value {
				font-size: 11px;
				color: var(--text-2, #adb5be);
			}
			.injection-preview-box {
				background: var(--surface, #161b22);
				border: 1px solid var(--rose-border, rgba(184, 132, 158, 0.15));
				border-radius: 3px;
				padding: 10px 12px;
				font-size: 11px;
				color: var(--text-2, #adb5be);
				white-space: pre-wrap;
				word-break: break-word;
				overflow-y: auto;
				max-height: 260px;
				line-height: 1.6;
			}
			.injection-preview-label {
				font-size: 10px;
				color: var(--rose, #b8849e);
				font-weight: 600;
				letter-spacing: 0.06em;
				text-transform: uppercase;
				margin-bottom: 4px;
			}

			.dry-run-summary-bar {
				background: var(--surface, #161b22);
				border-top: 1px solid var(--rose-border, rgba(184, 132, 158, 0.15));
				padding: 10px 14px;
				display: flex;
				align-items: center;
				gap: 16px;
				font-size: 11px;
			}
			.summary-bar-fires {
				color: var(--rose, #b8849e);
				font-weight: 600;
			}
			.summary-bar-passes {
				color: var(--text-3, #c0c7d0);
			}
			.summary-bar-sep {
				color: var(--border, rgba(194, 201, 209, 0.2));
			}

			.dry-run-action-row {
				display: flex;
				align-items: center;
				gap: 10px;
			}
			.gf-btn-enrol {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 7px 16px;
				border: 1px solid var(--rose-border, rgba(184, 132, 158, 0.15));
				border-radius: 4px;
				background: var(--rose-dim, rgba(184, 132, 158, 0.08));
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				font-weight: 600;
				color: var(--rose, #b8849e);
				cursor: pointer;
				transition: background 0.15s;
			}
			.gf-btn-refine {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 7px 14px;
				border: 1px solid var(--border, rgba(194, 201, 209, 0.12));
				border-radius: 4px;
				background: none;
				font-family: var(--font-mono, monospace);
				font-size: 11px;
				color: var(--text-2, #adb5be);
				cursor: pointer;
				transition: border-color 0.15s;
			}
			.gf-btn-rerun {
				font-size: 10px;
				color: var(--text-3, #c0c7d0);
				background: none;
				border: none;
				cursor: pointer;
				font-family: var(--font-mono, monospace);
				padding: 4px 8px;
				margin-left: auto;
			}

			.gf-footer-row {
				display: flex;
				align-items: center;
				justify-content: space-between;
				padding-top: 4px;
				border-top: 1px solid var(--border, rgba(194, 201, 209, 0.08));
			}
			.gf-btn-secondary {
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				background: none;
				border: none;
				cursor: pointer;
				font-family: var(--font-mono, monospace);
				padding: 4px 8px;
			}

			.annotation {
				margin-top: 24px;
				border: 1px solid var(--border, rgba(194, 201, 209, 0.1));
				border-radius: 4px;
				padding: 12px 16px;
				font-size: 11px;
				color: var(--text-2, #adb5be);
				background: var(--surface, #161b22);
				max-width: 780px;
				margin-left: auto;
				margin-right: auto;
			}
			.annotation strong {
				color: var(--text-1, #c2c9d1);
			}
			.annotation code {
				background: var(--surface-2, #1c232d);
				padding: 1px 4px;
				border-radius: 2px;
				font-size: 10px;
			}
		</style>

		<div class="gf-card">
			<div class="gf-card-header">
				<span class="gf-step-badge">STEP 3</span>
				<span class="gf-card-title">Craft the steer</span>
			</div>
			<div class="gf-card-body">
				<div class="dry-run-panel">
					<div class="dry-run-panel-header">
						<div class="dry-run-panel-header-left">
							<span class="dry-run-fire-badge">⚡ 3</span>
							Dry-run results — 3 of 5 turns would fire
						</div>
						<span class="dry-run-panel-header-right">tested 5 recent turns</span>
					</div>

					<div class="dry-run-panel-body">
						<div class="dry-run-turn-row">
							<div class="turn-row-header">
								<span class="turn-marker--pass">✓</span>
								<div class="turn-info">
									<span class="turn-label"
										><strong>Turn 1</strong> — user: "fix the failing migration test in persistence.rs"</span
									>
									<span class="turn-result--pass">Would NOT fire — confidence: 0.08</span>
								</div>
							</div>
						</div>

						<div class="dry-run-turn-row">
							<div class="turn-row-header">
								<span class="turn-marker--pass">✓</span>
								<div class="turn-info">
									<span class="turn-label"
										><strong>Turn 2</strong> — user: "the test is still failing, here's the cargo output"</span
									>
									<span class="turn-result--pass">Would NOT fire — confidence: 0.14</span>
								</div>
							</div>
						</div>

						<div class="dry-run-turn-row">
							<div class="turn-row-header">
								<span class="turn-marker--fire">⚡</span>
								<div class="turn-info">
									<span class="turn-label"
										><strong>Turn 3</strong> — user: "add the fleet integration test"</span
									>
									<span class="turn-result--fire">⚡ Would fire — confidence: 0.87</span>
								</div>
							</div>
							<div class="turn-detail">
								<div class="turn-detail-field">
									<span class="turn-detail-key">Command</span>
									<span class="turn-detail-value">redirect-attention</span>
								</div>
								<div class="turn-detail-field">
									<span class="turn-detail-key">Violation topic</span>
									<span class="turn-detail-value"
										>Session is writing an unrelated fleet integration test while the original
										migration test failure is unresolved. The assistant declared the original
										deliverable complete without verified output.</span
									>
								</div>
								<div class="turn-detail-field">
									<span class="injection-preview-label">Would inject</span>
									<!-- svelte-ignore a11y_no_noninteractive_tabindex -- the box scrolls, so it must take keyboard focus for its content to be reachable -->
									<div
										class="injection-preview-box"
										role="region"
										aria-label="Injection preview"
										tabindex="0"
									>
										[bigbro steerer] Session drift detected: the current task is writing a fleet
										integration test, but the original deliverable (fixing the failing migration
										test in crates/mike/tests/integration/persistence.rs) is unresolved. The session
										MUST NOT move on to unrelated work until the migration test passes. Do not claim
										the test is pre-existing or out-of-scope without evidence. Do not report "all
										tests pass" without showing actual nextest output. Return to the original task.
										The acceptance criterion is: `cargo nextest run -p mike --test persistence`
										passes with zero failures and the output is shown verbatim.
									</div>
								</div>
							</div>
						</div>

						<div class="dry-run-turn-row">
							<div class="turn-row-header">
								<span class="turn-marker--fire">⚡</span>
								<div class="turn-info">
									<span class="turn-label"
										><strong>Turn 4</strong> — user: "ok what about the steerer retry logic"</span
									>
									<span class="turn-result--fire">⚡ Would fire — confidence: 0.91</span>
								</div>
							</div>
						</div>

						<div class="dry-run-turn-row">
							<div class="turn-row-header">
								<span class="turn-marker--fire">⚡</span>
								<div class="turn-info">
									<span class="turn-label"
										><strong>Turn 5</strong> — user: "can you explain what a retry backoff does"</span
									>
									<span class="turn-result--fire">⚡ Would fire — confidence: 0.79</span>
								</div>
							</div>
						</div>
					</div>

					<div class="dry-run-summary-bar">
						<span class="summary-bar-fires">3 would fire</span>
						<span class="summary-bar-sep">·</span>
						<span class="summary-bar-passes">2 would not</span>
						<span class="summary-bar-sep">·</span>
						<span class="summary-bar-passes">avg confidence: 0.72</span>
					</div>
				</div>

				<div class="dry-run-action-row">
					<button class="gf-btn-enrol" type="button">Looks good — Enrol →</button>
					<button class="gf-btn-refine" type="button">Refine prompt ↑</button>
					<button class="gf-btn-rerun" type="button">⟳ Re-run</button>
				</div>

				<div class="gf-footer-row">
					<button class="gf-btn-secondary" type="button">← Back</button>
				</div>
			</div>
		</div>

		<div class="annotation">
			<strong>Inspection fidelity rule:</strong> The <code>injection_preview</code> box MUST NOT use
			<code>line-clamp</code>, <code>truncate</code>, or <code>overflow: hidden</code> with a fixed
			height. The preview is the diagnostic artifact — it renders in full with
			<code>white-space: pre-wrap</code>
			and <code>overflow-y: auto</code> so long text scrolls rather than clips.
			<br /><br />
			<strong>Action hierarchy:</strong> "Looks good — Enrol →" is the primary action (rose). "Refine
			prompt ↑" is secondary (neutral) and returns focus to the anchor textarea for editing. "⟳ Re-run"
			is tertiary (text-only) — re-tests the current anchor text after edits. When "Refine prompt ↑" is
			clicked, the dry-run panel collapses and the anchor textarea receives focus so the operator can
			revise the wording.
		</div>
	</div>
</Story>

<Story name="DryRun_Error">
	<div class="mock-step3-shell">
		<style>
			.mock-step3-shell {
				background: var(--bg, #0f1117);
				min-height: 100vh;
				padding: 32px;
				font-family: var(--font-mono, monospace);
				color: var(--text-1, #c2c9d1);
			}
			.gf-card {
				background: var(--surface, #161b22);
				border: 1px solid var(--border, rgba(194, 201, 209, 0.1));
				border-radius: 6px;
				max-width: 780px;
				margin: 0 auto;
				overflow: hidden;
			}
			.gf-card-header {
				padding: 14px 20px;
				border-bottom: 1px solid var(--border, rgba(194, 201, 209, 0.1));
				display: flex;
				align-items: center;
				gap: 10px;
			}
			.gf-step-badge {
				font-size: 10px;
				font-weight: 700;
				color: var(--rose, #b8849e);
				background: var(--rose-dim, rgba(184, 132, 158, 0.08));
				border: 1px solid var(--rose-border, rgba(184, 132, 158, 0.15));
				border-radius: 3px;
				padding: 2px 7px;
				letter-spacing: 0.06em;
			}
			.gf-card-title {
				font-size: 12px;
				font-weight: 600;
				color: var(--text-1, #c2c9d1);
			}
			.gf-card-body {
				padding: 20px;
				display: flex;
				flex-direction: column;
				gap: 16px;
			}

			.dry-run-error-panel {
				border: 1px solid rgba(194, 201, 209, 0.15);
				border-radius: 4px;
				overflow: hidden;
			}
			.dry-run-error-header {
				background: rgba(194, 201, 209, 0.05);
				padding: 10px 14px;
				display: flex;
				align-items: center;
				gap: 8px;
				font-size: 11px;
				font-weight: 600;
				color: var(--text-2, #adb5be);
			}
			.dry-run-error-body {
				background: var(--surface-2, #1c232d);
				padding: 12px 14px;
				display: flex;
				flex-direction: column;
				gap: 6px;
			}
			.error-notice {
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				background: var(--surface, #161b22);
				border: 1px solid rgba(194, 201, 209, 0.1);
				border-radius: 3px;
				padding: 8px 12px;
			}
			.error-turn-row {
				display: flex;
				align-items: flex-start;
				gap: 10px;
				padding: 7px 0;
				border-bottom: 1px solid var(--border, rgba(194, 201, 209, 0.06));
				font-size: 11px;
			}
			.error-turn-row:last-child {
				border-bottom: none;
			}
			.turn-marker--pass {
				color: var(--text-3, #c0c7d0);
				font-size: 13px;
				flex-shrink: 0;
				line-height: 1.4;
			}
			.turn-label {
				color: var(--text-2, #adb5be);
				flex: 1;
			}
			.turn-label strong {
				color: var(--text-1, #c2c9d1);
			}
			.turn-err {
				font-size: 10px;
				color: var(--text-3, #c0c7d0);
				white-space: nowrap;
			}

			.dry-run-timeout-row {
				display: flex;
				align-items: center;
				gap: 10px;
				padding: 8px 0;
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				border-bottom: 1px solid var(--border, rgba(194, 201, 209, 0.06));
			}
			.dry-run-timeout-row:last-child {
				border-bottom: none;
			}

			.dry-run-action-row {
				display: flex;
				align-items: center;
				gap: 10px;
			}
			.gf-btn-enrol {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 7px 16px;
				border: 1px solid var(--rose-border, rgba(184, 132, 158, 0.15));
				border-radius: 4px;
				background: var(--rose-dim, rgba(184, 132, 158, 0.08));
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				font-weight: 600;
				color: var(--rose, #b8849e);
				cursor: pointer;
			}
			.gf-btn-rerun {
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				background: none;
				border: 1px solid var(--border, rgba(194, 201, 209, 0.1));
				border-radius: 4px;
				cursor: pointer;
				font-family: var(--font-mono, monospace);
				padding: 6px 12px;
			}
			.gf-footer-row {
				display: flex;
				align-items: center;
				justify-content: space-between;
				padding-top: 4px;
				border-top: 1px solid var(--border, rgba(194, 201, 209, 0.08));
			}
			.gf-btn-secondary {
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				background: none;
				border: none;
				cursor: pointer;
				font-family: var(--font-mono, monospace);
				padding: 4px 8px;
			}

			.annotation {
				margin-top: 24px;
				border: 1px solid var(--border, rgba(194, 201, 209, 0.1));
				border-radius: 4px;
				padding: 12px 16px;
				font-size: 11px;
				color: var(--text-2, #adb5be);
				background: var(--surface, #161b22);
				max-width: 780px;
				margin-left: auto;
				margin-right: auto;
			}
			.annotation strong {
				color: var(--text-1, #c2c9d1);
			}
		</style>

		<div class="gf-card">
			<div class="gf-card-header">
				<span class="gf-step-badge">STEP 3</span>
				<span class="gf-card-title">Craft the steer</span>
			</div>
			<div class="gf-card-body">
				<div class="dry-run-error-panel">
					<div class="dry-run-error-header">⚠ Dry-run — partial results (timed out after 15 s)</div>
					<div class="dry-run-error-body">
						<div class="error-notice">
							The dry-run request timed out before all turns completed. 1 of 5 turns evaluated. You
							can re-run or enrol anyway — the steer will behave correctly at runtime.
						</div>

						<div class="error-turn-row">
							<span class="turn-marker--pass">✓</span>
							<span class="turn-label"
								><strong>Turn 1</strong> — user: "fix the failing migration test"</span
							>
							<span class="turn-err">confidence: 0.08 — would NOT fire</span>
						</div>

						<div class="dry-run-timeout-row">
							<span style="color: var(--border, rgba(194,201,209,.3)); font-size: 12px;">—</span>
							<span>Turn 2 — timed out</span>
						</div>
						<div class="dry-run-timeout-row">
							<span style="color: var(--border, rgba(194,201,209,.3)); font-size: 12px;">—</span>
							<span>Turn 3 — timed out</span>
						</div>
						<div class="dry-run-timeout-row">
							<span style="color: var(--border, rgba(194,201,209,.3)); font-size: 12px;">—</span>
							<span>Turn 4 — timed out</span>
						</div>
						<div class="dry-run-timeout-row">
							<span style="color: var(--border, rgba(194,201,209,.3)); font-size: 12px;">—</span>
							<span>Turn 5 — timed out</span>
						</div>
					</div>
				</div>

				<div class="dry-run-action-row">
					<button class="gf-btn-rerun" type="button">⟳ Re-run</button>
					<button class="gf-btn-enrol" type="button">Enrol anyway →</button>
				</div>

				<div class="gf-footer-row">
					<button class="gf-btn-secondary" type="button">← Back</button>
				</div>
			</div>
		</div>

		<div class="annotation">
			<strong>Graceful degradation:</strong> Timeout is not fatal. The operator can still enrol the steer
			— it will work correctly at runtime regardless of whether the dry-run succeeded. The error state
			shows how many turns were evaluated and the raw error (timeout vs. server error). "Re-run" re-POSTs
			to the same endpoint. "Enrol anyway →" proceeds directly to confirmation.
		</div>
	</div>
</Story>

<Story name="DryRun_NoTurns">
	<div class="mock-step3-shell">
		<style>
			.mock-step3-shell {
				background: var(--bg, #0f1117);
				min-height: 100vh;
				padding: 32px;
				font-family: var(--font-mono, monospace);
				color: var(--text-1, #c2c9d1);
			}
			.gf-card {
				background: var(--surface, #161b22);
				border: 1px solid var(--border, rgba(194, 201, 209, 0.1));
				border-radius: 6px;
				max-width: 780px;
				margin: 0 auto;
				overflow: hidden;
			}
			.gf-card-header {
				padding: 14px 20px;
				border-bottom: 1px solid var(--border, rgba(194, 201, 209, 0.1));
				display: flex;
				align-items: center;
				gap: 10px;
			}
			.gf-step-badge {
				font-size: 10px;
				font-weight: 700;
				color: var(--rose, #b8849e);
				background: var(--rose-dim, rgba(184, 132, 158, 0.08));
				border: 1px solid var(--rose-border, rgba(184, 132, 158, 0.15));
				border-radius: 3px;
				padding: 2px 7px;
				letter-spacing: 0.06em;
			}
			.gf-card-title {
				font-size: 12px;
				font-weight: 600;
				color: var(--text-1, #c2c9d1);
			}
			.gf-card-body {
				padding: 20px;
				display: flex;
				flex-direction: column;
				gap: 16px;
			}

			.dry-run-empty {
				border: 1px solid var(--border, rgba(194, 201, 209, 0.1));
				border-radius: 4px;
				padding: 16px;
				text-align: center;
				background: var(--surface-2, #1c232d);
			}
			.dry-run-empty-icon {
				font-size: 22px;
				margin-bottom: 8px;
				opacity: 0.5;
			}
			.dry-run-empty-title {
				font-size: 12px;
				font-weight: 600;
				color: var(--text-2, #adb5be);
				margin-bottom: 4px;
			}
			.dry-run-empty-desc {
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				line-height: 1.6;
			}

			.dry-run-btn--disabled {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 6px 14px;
				border: 1px solid var(--border, rgba(194, 201, 209, 0.08));
				border-radius: 4px;
				background: none;
				font-family: var(--font-mono, monospace);
				font-size: 11px;
				font-weight: 600;
				color: var(--text-3, #c0c7d0);
				cursor: not-allowed;
				opacity: 0.45;
			}
			.gf-btn-enrol {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 7px 16px;
				border: 1px solid var(--rose-border, rgba(184, 132, 158, 0.15));
				border-radius: 4px;
				background: var(--rose-dim, rgba(184, 132, 158, 0.08));
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				font-weight: 600;
				color: var(--rose, #b8849e);
				cursor: pointer;
			}
			.gf-footer-row {
				display: flex;
				align-items: center;
				justify-content: space-between;
				padding-top: 4px;
				border-top: 1px solid var(--border, rgba(194, 201, 209, 0.08));
			}
			.gf-btn-secondary {
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				background: none;
				border: none;
				cursor: pointer;
				font-family: var(--font-mono, monospace);
				padding: 4px 8px;
			}
			.dry-run-action-row {
				display: flex;
				align-items: center;
				gap: 10px;
			}
			.annotation {
				margin-top: 24px;
				border: 1px solid var(--border, rgba(194, 201, 209, 0.1));
				border-radius: 4px;
				padding: 12px 16px;
				font-size: 11px;
				color: var(--text-2, #adb5be);
				background: var(--surface, #161b22);
				max-width: 780px;
				margin-left: auto;
				margin-right: auto;
			}
			.annotation strong {
				color: var(--text-1, #c2c9d1);
			}
		</style>

		<div class="gf-card">
			<div class="gf-card-header">
				<span class="gf-step-badge">STEP 3</span>
				<span class="gf-card-title">Craft the steer</span>
			</div>
			<div class="gf-card-body">
				<div class="dry-run-empty">
					<div class="dry-run-empty-icon">⟳</div>
					<div class="dry-run-empty-title">No prior turns to test against</div>
					<div class="dry-run-empty-desc">
						This session has no evaluated turns yet. The steer will activate on future turns once
						enrolled. You can enrol now and the steerer will begin evaluating from the next turn.
					</div>
				</div>

				<div class="dry-run-action-row">
					<button
						class="dry-run-btn--disabled"
						type="button"
						disabled
						title="No prior turns available"
					>
						Test this steer →
					</button>
					<button class="gf-btn-enrol" type="button">Enrol →</button>
				</div>

				<div class="gf-footer-row">
					<button class="gf-btn-secondary" type="button">← Back</button>
				</div>
			</div>
		</div>

		<div class="annotation">
			<strong>Empty session state:</strong> When there are no prior turns (new session or
			anchor_request_id points to the first response), "Test this steer →" is disabled (<code
				>cursor: not-allowed</code
			>,
			<code>opacity: 0.45</code>). The operator proceeds directly to Enrol. This is distinct from
			the error state — it's not a failure, just no data to test against.
		</div>
	</div>
</Story>
