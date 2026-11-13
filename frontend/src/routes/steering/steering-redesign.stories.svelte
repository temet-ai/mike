<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { resolve } from '$app/paths';
	import EmptyStateOnboarding from '$lib/components/shared/EmptyStateOnboarding.svelte';

	const { Story } = defineMeta({
		title: 'Draft/SteeringRedesign',
		component: EmptyStateOnboarding,
		tags: ['autodocs'],
		parameters: {
			layout: 'fullscreen',
			docs: {
				description: {
					component:
						'Visual spec for the Steering Hub UX redesign. ' +
						'Static HTML — not the live pages. ' +
						'See artifacts/2026-06-15/steering-ux-redesign-spec.md.'
				}
			}
		}
	});
</script>

<Story
	name="CreateFlowEmpty"
	parameters={{
		docs: {
			description: {
				story:
					'New steerer create flow: editable display name, slug field, onboarding explanation, working Create button. Slug is immutable after creation.'
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

			.se-header {
				display: flex;
				align-items: center;
				gap: 12px;
				padding: 16px 24px;
				border-bottom: 1px solid var(--border-color, #30363d);
				flex-shrink: 0;
			}
			.se-back {
				display: flex;
				align-items: center;
				justify-content: center;
				width: 28px;
				height: 28px;
				border-radius: 4px;
				border: 1px solid var(--border-subtle, #23282f);
				color: var(--text-2, #adb5be);
				text-decoration: none;
				flex-shrink: 0;
				font-size: 14px;
			}
			.se-back:hover {
				border-color: var(--status-steering-border);
				color: var(--status-steering-fg);
			}
			.se-title {
				margin: 0;
				font-size: 1.05rem;
				font-weight: 600;
				color: var(--text-strong, #dde2e8);
			}

			.se-body {
				max-width: 680px;
				margin: 0 auto;
				padding: 24px 24px 48px;
				display: flex;
				flex-direction: column;
				gap: 20px;
			}

			.se-onboarding {
				padding: 14px 16px;
				background: var(--surface-2, #1c232d);
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 6px;
				font-size: 12px;
				color: var(--text-2, #adb5be);
				line-height: 1.6;
			}

			.se-fieldset {
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 6px;
				padding: 14px 16px;
				display: flex;
				flex-direction: column;
				gap: 14px;
				min-inline-size: 0;
			}
			.se-legend {
				font-family: var(--font-mono, monospace);
				font-size: 10px;
				font-weight: 600;
				text-transform: uppercase;
				letter-spacing: 0.06em;
				color: var(--text-3, #c0c7d0);
				padding: 0 4px;
			}
			.se-field {
				display: flex;
				flex-direction: column;
				gap: 4px;
			}
			.se-field--row {
				flex-direction: row;
				align-items: center;
				gap: 10px;
			}
			.se-label {
				font-family: var(--font-mono, monospace);
				font-size: 11px;
				font-weight: 600;
				color: var(--text-2, #adb5be);
				display: flex;
				align-items: center;
				gap: 6px;
			}
			.se-label__note {
				font-weight: 400;
				color: var(--text-3, #c0c7d0);
			}
			.se-label__help {
				width: 12px;
				height: 12px;
				color: var(--text-3, #c0c7d0);
				cursor: help;
				flex-shrink: 0;
			}
			.se-input {
				padding: 6px 8px;
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 4px;
				background: var(--surface-2, #1c232d);
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				width: 100%;
				box-sizing: border-box;
			}
			.se-input:focus {
				outline: 1px solid var(--status-steering-border);
				border-color: var(--status-steering-border);
			}
			.se-input--slug {
				font-style: italic;
				color: var(--text-2, #adb5be);
			}
			.se-field__help {
				font-family: var(--font-mono, monospace);
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				margin: 0;
				line-height: 1.4;
			}
			.se-textarea {
				padding: 6px 8px;
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 4px;
				background: var(--surface-2, #1c232d);
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				width: 100%;
				box-sizing: border-box;
				resize: vertical;
				min-height: 64px;
			}

			.se-toggle {
				background: none;
				border: none;
				cursor: pointer;
				padding: 2px;
				display: flex;
				align-items: center;
				gap: 8px;
			}
			.se-toggle__track {
				display: block;
				position: relative;
				width: 32px;
				height: 18px;
				border-radius: 9px;
				background: var(--surface-2, #1c232d);
				border: 1px solid var(--border-subtle, #23282f);
				flex-shrink: 0;
			}
			.se-toggle__thumb {
				display: block;
				position: absolute;
				top: 1px;
				left: 1px;
				width: 14px;
				height: 14px;
				border-radius: 50%;
				background: var(--text-3, #c0c7d0);
			}
			.se-toggle__label {
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				color: var(--text-2, #adb5be);
			}

			.se-select {
				padding: 6px 8px;
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 4px;
				background: var(--surface-2, #1c232d);
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				width: 100%;
				cursor: pointer;
			}

			.se-footer {
				display: flex;
				align-items: center;
				gap: 12px;
				padding-top: 8px;
				border-top: 1px solid var(--border-subtle, #23282f);
			}
			.se-btn {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 7px 16px;
				border-radius: 4px;
				font-size: 12px;
				font-family: var(--font-mono, monospace);
				font-weight: 500;
				text-decoration: none;
				border: 1px solid;
				cursor: pointer;
			}
			.se-btn--create {
				color: var(--status-steering-fg);
				border-color: var(--status-steering-border);
				background: var(--status-steering-bg);
			}
			.se-btn--create:hover {
				background: color-mix(
					in srgb,
					var(--status-steering-bg) 70%,
					var(--status-steering-fg) 30%
				);
			}
			.se-footer__note {
				font-family: var(--font-mono, monospace);
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
			}

			.se-tooltip-mock {
				display: inline-flex;
				align-items: center;
				justify-content: center;
				width: 12px;
				height: 12px;
				border-radius: 50%;
				border: 1px solid var(--border-color, #30363d);
				color: var(--text-3, #c0c7d0);
				font-size: 9px;
				font-weight: 700;
				cursor: help;
				flex-shrink: 0;
				line-height: 1;
			}
		</style>

		<header class="se-header">
			<a href={resolve('/steering?tab=fleet')} class="se-back" aria-label="Back to fleet">←</a>
			<div>
				<h2 class="se-title">New steerer</h2>
			</div>
		</header>

		<div class="se-body">
			<div class="se-onboarding" role="note">
				You are creating a steerer — a named agent configuration that monitors enrolled sessions for
				drift and injects steering messages when triggered. Give it a name and slug now (the slug
				cannot be changed later). You will be able to edit all other settings after creation.
			</div>

			<fieldset class="se-fieldset">
				<legend class="se-legend">Basic information</legend>

				<div class="se-field">
					<label class="se-label" for="c-display-name">
						Display name
						<span
							class="se-tooltip-mock"
							aria-hidden="true"
							title="Human-readable label for this steerer.">?</span
						>
					</label>
					<input
						id="c-display-name"
						type="text"
						class="se-input"
						value="Refocus"
						placeholder="e.g. Refocus"
					/>
				</div>

				<div class="se-field">
					<label class="se-label" for="c-slug">
						Slug
						<span class="se-label__note">(internal identifier, immutable after creation)</span>
					</label>
					<input
						id="c-slug"
						type="text"
						class="se-input se-input--slug"
						value="refocus"
						placeholder="e.g. refocus"
						pattern="[a-z0-9-]+"
					/>
					<p class="se-field__help">
						Used internally to identify this steerer. Cannot be changed after creation. Allowed
						characters: <code>a-z 0-9 -</code>
					</p>
				</div>

				<div class="se-field">
					<label class="se-label" for="c-description">Description</label>
					<textarea
						id="c-description"
						class="se-textarea"
						rows={3}
						placeholder="What does this steerer do?"
						>Brings the session back to its thread anchor when it wanders off topic.</textarea
					>
				</div>

				<div class="se-field se-field--row">
					<label class="se-label" for="c-enabled">
						Enabled
						<span
							class="se-tooltip-mock"
							aria-hidden="true"
							title="Whether this steerer evaluates enrolled sessions.">?</span
						>
					</label>
					<button type="button" class="se-toggle" aria-pressed="false" aria-label="Enable steerer">
						<span class="se-toggle__track" aria-hidden="true">
							<span class="se-toggle__thumb"></span>
						</span>
						<span class="se-toggle__label">Disabled</span>
					</button>
				</div>
			</fieldset>

			<fieldset class="se-fieldset">
				<legend class="se-legend">Abilities</legend>

				<div class="se-field">
					<label class="se-label" for="c-authority">
						Authority
						<span
							class="se-tooltip-mock"
							aria-hidden="true"
							title="How forceful the steerer's messages are.">?</span
						>
					</label>
					<select id="c-authority" class="se-select">
						<option value="supply_context" selected>supply context — adds background info</option>
						<option value="redirect">redirect — suggests a change of direction</option>
						<option value="command">command — binding directive</option>
					</select>
				</div>

				<div class="se-field">
					<label class="se-label" for="c-mode">
						Mode
						<span
							class="se-tooltip-mock"
							aria-hidden="true"
							title="Whether the steerer nudges or takes the wheel.">?</span
						>
					</label>
					<select id="c-mode" class="se-select">
						<option value="augment" selected>augment — adds alongside existing context</option>
						<option value="drive">drive — overrides next turn</option>
					</select>
				</div>
			</fieldset>

			<div class="se-footer">
				<button type="button" class="se-btn se-btn--create"> Create steerer </button>
				<span class="se-footer__note"> You will be redirected to the editor after creation. </span>
			</div>
		</div>
	</div>
</Story>

<Story
	name="EditorWithTooltips"
	parameters={{
		docs: {
			description: {
				story:
					'Edit page with tooltip help icons on every field. Slug shown as read-only. Edit prompt and Delete buttons in header. No warning banner.'
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
			.se-header {
				display: flex;
				align-items: center;
				gap: 12px;
				padding: 16px 24px;
				border-bottom: 1px solid var(--border-color, #30363d);
			}
			.se-back {
				display: flex;
				align-items: center;
				justify-content: center;
				width: 28px;
				height: 28px;
				border-radius: 4px;
				border: 1px solid var(--border-subtle, #23282f);
				color: var(--text-2, #adb5be);
				text-decoration: none;
				font-size: 14px;
			}
			.se-header__titles {
				display: flex;
				flex-direction: column;
				gap: 2px;
				flex: 1;
				min-width: 0;
			}
			.se-title {
				margin: 0;
				font-size: 1.05rem;
				font-weight: 600;
				color: var(--text-strong, #dde2e8);
			}
			.se-slug-label {
				font-family: var(--font-mono, monospace);
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
			}
			.se-header__actions {
				display: flex;
				align-items: center;
				gap: 8px;
				flex-shrink: 0;
			}
			.se-btn {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 5px 10px;
				border-radius: 4px;
				font-size: 12px;
				font-family: var(--font-mono, monospace);
				font-weight: 500;
				text-decoration: none;
				border: 1px solid var(--border-color, #30363d);
				cursor: pointer;
				color: var(--text-1, #c2c9d1);
				background: var(--surface-2, #1c232d);
			}
			.se-btn--danger {
				color: var(--status-danger-fg);
				border-color: rgba(196, 128, 128, 0.2);
			}
			.se-btn--save {
				color: var(--status-steering-fg);
				border-color: var(--status-steering-border);
				background: var(--status-steering-bg);
			}
			.se-body {
				max-width: 680px;
				margin: 0 auto;
				padding: 24px 24px 48px;
				display: flex;
				flex-direction: column;
				gap: 20px;
			}
			.se-fieldset {
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 6px;
				padding: 14px 16px;
				display: flex;
				flex-direction: column;
				gap: 14px;
				min-inline-size: 0;
			}
			.se-legend {
				font-family: var(--font-mono, monospace);
				font-size: 10px;
				font-weight: 600;
				text-transform: uppercase;
				letter-spacing: 0.06em;
				color: var(--text-3, #c0c7d0);
				padding: 0 4px;
			}
			.se-field {
				display: flex;
				flex-direction: column;
				gap: 4px;
			}
			.se-field--row {
				flex-direction: row;
				align-items: center;
				gap: 10px;
			}
			.se-label {
				font-family: var(--font-mono, monospace);
				font-size: 11px;
				font-weight: 600;
				color: var(--text-2, #adb5be);
				display: flex;
				align-items: center;
				gap: 6px;
			}
			.se-label__note {
				font-weight: 400;
				color: var(--text-3, #c0c7d0);
			}
			.se-label__help-icon {
				display: inline-flex;
				align-items: center;
				justify-content: center;
				width: 13px;
				height: 13px;
				border-radius: 50%;
				border: 1px solid var(--border-color, #30363d);
				color: var(--text-3, #c0c7d0);
				font-size: 9px;
				font-weight: 700;
				cursor: help;
				flex-shrink: 0;
				line-height: 1;
			}
			.se-tooltip-bubble {
				display: inline-block;
				padding: 4px 8px;
				background: var(--surface-2, #1c232d);
				border: 1px solid var(--border-color, #30363d);
				border-radius: 4px;
				font-size: 11px;
				color: var(--text-1, #c2c9d1);
				max-width: 260px;
				line-height: 1.4;
				margin-top: 4px;
			}
			.se-input {
				padding: 6px 8px;
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 4px;
				background: var(--surface-2, #1c232d);
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				width: 100%;
				box-sizing: border-box;
			}
			.se-input--narrow {
				max-width: 140px;
			}
			.se-textarea {
				padding: 6px 8px;
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 4px;
				background: var(--surface-2, #1c232d);
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				width: 100%;
				box-sizing: border-box;
				resize: vertical;
				min-height: 64px;
			}
			.se-select {
				padding: 6px 8px;
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 4px;
				background: var(--surface-2, #1c232d);
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				width: 100%;
				cursor: pointer;
			}
			.se-toggle {
				background: none;
				border: none;
				cursor: pointer;
				padding: 2px;
				display: flex;
				align-items: center;
				gap: 8px;
			}
			.se-toggle__track {
				display: block;
				position: relative;
				width: 32px;
				height: 18px;
				border-radius: 9px;
				background: var(--status-steering-bg);
				border: 1px solid var(--status-steering-border);
				flex-shrink: 0;
			}
			.se-toggle__thumb--on {
				display: block;
				position: absolute;
				top: 1px;
				left: 15px;
				width: 14px;
				height: 14px;
				border-radius: 50%;
				background: var(--status-steering-fg);
			}
			.se-toggle__label {
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				color: var(--text-2, #adb5be);
			}
			.se-footer {
				display: flex;
				align-items: center;
				gap: 12px;
				padding-top: 8px;
				border-top: 1px solid var(--border-subtle, #23282f);
			}
			.se-footer__note {
				font-family: var(--font-mono, monospace);
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
			}
			.se-model-row {
				display: flex;
				gap: 8px;
				align-items: flex-start;
			}
			.se-model-row .se-input {
				flex: 1;
			}
			.se-btn--picker {
				flex-shrink: 0;
				padding: 6px 10px;
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 4px;
				background: var(--surface-2, #1c232d);
				color: var(--text-2, #adb5be);
				font-family: var(--font-mono, monospace);
				font-size: 11px;
				cursor: pointer;
				white-space: nowrap;
			}
			.se-btn--picker:hover {
				border-color: var(--status-steering-border);
				color: var(--status-steering-fg);
			}
		</style>

		<header class="se-header">
			<a href={resolve('/steering?tab=fleet')} class="se-back" aria-label="Back">←</a>
			<div class="se-header__titles">
				<h2 class="se-title">Refocus</h2>
				<span class="se-slug-label">refocus</span>
			</div>
			<div class="se-header__actions">
				<a href={resolve('agents/42')} class="se-btn" target="_blank" rel="noopener">
					↗ Edit prompt
				</a>
				<button type="button" class="se-btn se-btn--danger"> Delete </button>
			</div>
		</header>

		<div class="se-body">
			<fieldset class="se-fieldset">
				<legend class="se-legend">Basic information</legend>

				<div class="se-field">
					<label class="se-label" for="e-display-name">
						Display name
						<span
							class="se-label__help-icon"
							aria-hidden="true"
							title="Human-readable label shown in the Fleet table and Monitor feed.">?</span
						>
					</label>
					<input id="e-display-name" type="text" class="se-input" value="Refocus" />
				</div>

				<div class="se-field">
					<label class="se-label" for="e-description">Description</label>
					<textarea id="e-description" class="se-textarea" rows={3}
						>Brings the session back to its thread anchor when it wanders off topic.</textarea
					>
				</div>

				<div class="se-field se-field--row">
					<label class="se-label" for="e-enabled">
						Enabled
						<span
							class="se-label__help-icon"
							aria-hidden="true"
							title="Whether this steerer evaluates enrolled sessions.">?</span
						>
					</label>
					<button type="button" class="se-toggle" aria-pressed="true" aria-label="Disable steerer">
						<span class="se-toggle__track" aria-hidden="true">
							<span class="se-toggle__thumb--on"></span>
						</span>
						<span class="se-toggle__label">Enabled</span>
					</button>
				</div>
			</fieldset>

			<fieldset class="se-fieldset">
				<legend class="se-legend">Abilities</legend>

				<div class="se-field">
					<label class="se-label" for="e-authority">
						Authority
						<span
							class="se-label__help-icon"
							aria-hidden="true"
							title="How forceful the steerer's messages are. supply context &lt; redirect &lt; command (binding)."
							>?</span
						>
					</label>
					<div class="se-tooltip-bubble" aria-hidden="true" role="presentation">
						How forceful the steerer's messages are. Ranges from context-only (weakest) to binding
						command (strongest).
					</div>
					<select id="e-authority" class="se-select">
						<option value="supply_context" selected>supply context — adds background info</option>
						<option value="redirect">redirect — suggests a change of direction</option>
						<option value="command">command — binding directive</option>
					</select>
				</div>

				<div class="se-field">
					<label class="se-label" for="e-mode">
						Mode
						<span
							class="se-label__help-icon"
							aria-hidden="true"
							title="Whether the steerer nudges (augment) or overrides the next turn (drive)."
							>?</span
						>
					</label>
					<select id="e-mode" class="se-select">
						<option value="augment" selected>augment — adds alongside existing context</option>
						<option value="drive">drive — overrides next turn</option>
					</select>
				</div>
			</fieldset>

			<fieldset class="se-fieldset">
				<legend class="se-legend">Evaluation config</legend>

				<div class="se-field">
					<label class="se-label" for="e-model">
						Model
						<span
							class="se-label__help-icon"
							aria-hidden="true"
							title="OpenRouter model for drift detection. Smaller models (gpt-5.6-luna, gemini-3.5-flash-lite) are sufficient for most steering."
							>?</span
						>
					</label>
					<div class="se-model-row">
						<input id="e-model" type="text" class="se-input" value="openai/gpt-5.6-luna" readonly />
						<button type="button" class="se-btn--picker">Select ▼</button>
					</div>
				</div>

				<div class="se-field">
					<label class="se-label" for="e-confidence">
						Confidence threshold
						<span
							class="se-label__help-icon"
							aria-hidden="true"
							title="Score (0–1) the drift detector must reach before firing. Higher = less sensitive."
							>?</span
						>
						<span class="se-label__note">0.0 – 1.0 · fires when score ≥ threshold</span>
					</label>
					<input
						id="e-confidence"
						type="number"
						class="se-input se-input--narrow"
						value="0.7"
						min="0"
						max="1"
						step="0.01"
					/>
				</div>

				<div class="se-field">
					<label class="se-label" for="e-debounce">
						Debounce turns
						<span
							class="se-label__help-icon"
							aria-hidden="true"
							title="Minimum turns after a fire before this steerer can fire again.">?</span
						>
						<span class="se-label__note">turns · minimum gap between fires</span>
					</label>
					<input
						id="e-debounce"
						type="number"
						class="se-input se-input--narrow"
						value="2"
						min="0"
						step="1"
					/>
				</div>

				<div class="se-field">
					<label class="se-label" for="e-priority">
						Priority
						<span
							class="se-label__help-icon"
							aria-hidden="true"
							title="Evaluation order when multiple steerers watch the same session. Lower numbers run first."
							>?</span
						>
						<span class="se-label__note">lower = evaluated first</span>
					</label>
					<input
						id="e-priority"
						type="number"
						class="se-input se-input--narrow"
						value="100"
						min="1"
						step="1"
					/>
				</div>
			</fieldset>

			<div class="se-footer">
				<button type="button" class="se-btn se-btn--save"> Save changes </button>
				<span class="se-footer__note">
					Authority and mode changes take effect on the next evaluation.
				</span>
			</div>
		</div>
	</div>
</Story>

<Story
	name="ModelPickerOpen"
	parameters={{
		docs: {
			description: {
				story:
					'Model picker popover open. Columns: Model · Context · $/M input · $/M output. Sorted cheapest first. Common models pinned at top. Search input.'
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
			.mp-demo {
				max-width: 580px;
				margin: 40px auto;
				position: relative;
			}
			.mp-label {
				font-size: 11px;
				color: var(--text-2, #adb5be);
				margin-bottom: 8px;
			}
			.mp-trigger-row {
				display: flex;
				gap: 8px;
				align-items: center;
				margin-bottom: 8px;
			}
			.mp-input {
				flex: 1;
				padding: 6px 8px;
				border: 1px solid var(--status-steering-border);
				border-radius: 4px;
				background: var(--surface-2, #1c232d);
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-mono, monospace);
				font-size: 12px;
			}
			.mp-btn-open {
				padding: 6px 10px;
				border: 1px solid var(--status-steering-border);
				border-radius: 4px;
				background: var(--status-steering-bg);
				color: var(--status-steering-fg);
				font-family: var(--font-mono, monospace);
				font-size: 11px;
				cursor: pointer;
				white-space: nowrap;
			}
			.mp-popover {
				position: relative;
				width: 520px;
				max-height: 360px;
				background: var(--surface, #161b22);
				border: 1px solid var(--border-color, #30363d);
				border-radius: 6px;
				box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
				display: flex;
				flex-direction: column;
				overflow: hidden;
			}
			.mp-search {
				padding: 10px 12px;
				border-bottom: 1px solid var(--border-subtle, #23282f);
				flex-shrink: 0;
			}
			.mp-search-input {
				width: 100%;
				box-sizing: border-box;
				padding: 5px 8px;
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 4px;
				background: var(--surface-2, #1c232d);
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-mono, monospace);
				font-size: 12px;
			}
			.mp-search-input::placeholder {
				color: var(--text-3, #c0c7d0);
			}
			.mp-col-headers {
				display: grid;
				grid-template-columns: 1fr 72px 72px 72px;
				gap: 0 8px;
				padding: 6px 12px;
				border-bottom: 1px solid var(--border-subtle, #23282f);
				flex-shrink: 0;
			}
			.mp-col-header {
				font-family: var(--font-mono, monospace);
				font-size: 10px;
				font-weight: 600;
				text-transform: uppercase;
				letter-spacing: 0.06em;
				color: var(--text-3, #c0c7d0);
				cursor: pointer;
			}
			.mp-col-header:hover {
				color: var(--text-1, #c2c9d1);
			}
			.mp-col-header--right {
				text-align: right;
			}
			.mp-section-label {
				padding: 8px 12px 4px;
				font-size: 10px;
				font-weight: 600;
				text-transform: uppercase;
				letter-spacing: 0.06em;
				color: var(--text-3, #c0c7d0);
				flex-shrink: 0;
			}
			.mp-list {
				overflow-y: auto;
				flex: 1;
				min-height: 0;
			}
			.mp-row {
				display: grid;
				grid-template-columns: 1fr 72px 72px 72px;
				gap: 0 8px;
				padding: 7px 12px;
				align-items: center;
				cursor: pointer;
				border-bottom: 1px solid var(--border-subtle, #23282f);
			}
			.mp-row:hover {
				background: color-mix(in srgb, var(--surface-2, #1c232d) 60%, transparent 40%);
			}
			.mp-row--selected {
				background: var(--status-steering-bg);
			}
			.mp-model-name {
				display: flex;
				flex-direction: column;
				gap: 1px;
				min-width: 0;
			}
			.mp-model-id {
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				color: var(--text-1, #c2c9d1);
			}
			.mp-model-provider {
				font-family: var(--font-mono, monospace);
				font-size: 10px;
				color: var(--text-3, #c0c7d0);
			}
			.mp-stat {
				font-family: var(--font-mono, monospace);
				font-size: 11px;
				color: var(--text-2, #adb5be);
				text-align: right;
			}
			.mp-stat--ctx {
				color: var(--text-3, #c0c7d0);
			}
			.mp-selected-dot {
				display: inline-block;
				width: 6px;
				height: 6px;
				border-radius: 50%;
				background: var(--status-steering-fg);
				margin-right: 6px;
				flex-shrink: 0;
				vertical-align: middle;
			}
			.mp-divider {
				height: 1px;
				background: var(--border-subtle, #23282f);
				margin: 0;
			}
		</style>

		<div class="mp-demo">
			<p class="mp-label">Model field with picker open (spec §4):</p>

			<div class="mp-trigger-row">
				<input class="mp-input" type="text" value="openai/gpt-5.6-luna" readonly />
				<button type="button" class="mp-btn-open">Select ▲</button>
			</div>

			<div class="mp-popover" role="dialog" aria-label="Select model">
				<div class="mp-search">
					<input type="text" class="mp-search-input" placeholder="filter by name or provider…" />
				</div>

				<div class="mp-col-headers" role="row">
					<span class="mp-col-header" role="columnheader">Model</span>
					<span class="mp-col-header mp-col-header--right" role="columnheader">Context</span>
					<span class="mp-col-header mp-col-header--right" role="columnheader">$/M in</span>
					<span class="mp-col-header mp-col-header--right" role="columnheader">$/M out</span>
				</div>

				<div class="mp-list" role="listbox" aria-label="Available models">
					<div class="mp-section-label" aria-hidden="true">Common for steering</div>

					<div class="mp-row mp-row--selected" role="option" aria-selected="true">
						<div class="mp-model-name">
							<span class="mp-model-id">
								<span class="mp-selected-dot" aria-hidden="true"></span>
								gpt-5.6-luna
							</span>
							<span class="mp-model-provider">openai</span>
						</div>
						<span class="mp-stat mp-stat--ctx">128K</span>
						<span class="mp-stat">$0.15</span>
						<span class="mp-stat">$0.60</span>
					</div>

					<div class="mp-row" role="option" aria-selected="false">
						<div class="mp-model-name">
							<span class="mp-model-id">gemini-3.5-flash-lite</span>
							<span class="mp-model-provider">google</span>
						</div>
						<span class="mp-stat mp-stat--ctx">200K</span>
						<span class="mp-stat">$0.80</span>
						<span class="mp-stat">$4.00</span>
					</div>

					<div class="mp-row" role="option" aria-selected="false">
						<div class="mp-model-name">
							<span class="mp-model-id">gemini-3.8-flash</span>
							<span class="mp-model-provider">google</span>
						</div>
						<span class="mp-stat mp-stat--ctx">200K</span>
						<span class="mp-stat">$3.00</span>
						<span class="mp-stat">$15.00</span>
					</div>

					<div class="mp-divider" role="separator"></div>
					<div class="mp-section-label" aria-hidden="true">All models — sorted by cost</div>

					<div class="mp-row" role="option" aria-selected="false">
						<div class="mp-model-name">
							<span class="mp-model-id">gemini-flash-1.5</span>
							<span class="mp-model-provider">google</span>
						</div>
						<span class="mp-stat mp-stat--ctx">1M</span>
						<span class="mp-stat">$0.075</span>
						<span class="mp-stat">$0.30</span>
					</div>

					<div class="mp-row" role="option" aria-selected="false">
						<div class="mp-model-name">
							<span class="mp-model-id">gpt-5.6-luna</span>
							<span class="mp-model-provider">openai</span>
						</div>
						<span class="mp-stat mp-stat--ctx">128K</span>
						<span class="mp-stat">$0.15</span>
						<span class="mp-stat">$0.60</span>
					</div>

					<div class="mp-row" role="option" aria-selected="false">
						<div class="mp-model-name">
							<span class="mp-model-id">mistral-7b-instruct</span>
							<span class="mp-model-provider">mistralai</span>
						</div>
						<span class="mp-stat mp-stat--ctx">32K</span>
						<span class="mp-stat">$0.07</span>
						<span class="mp-stat">$0.07</span>
					</div>
				</div>
			</div>
		</div>
	</div>
</Story>

<Story
	name="ModelPickerLoading"
	parameters={{
		docs: {
			description: {
				story:
					'Model picker in loading state: 3 skeleton rows pulse-animate while the /api/models endpoint is in flight.'
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
			.mp-demo {
				max-width: 580px;
				margin: 40px auto;
			}
			.mp-label {
				font-size: 11px;
				color: var(--text-2, #adb5be);
				margin-bottom: 8px;
			}
			.mp-popover {
				width: 520px;
				background: var(--surface, #161b22);
				border: 1px solid var(--border-color, #30363d);
				border-radius: 6px;
				display: flex;
				flex-direction: column;
				overflow: hidden;
			}
			.mp-search {
				padding: 10px 12px;
				border-bottom: 1px solid var(--border-subtle, #23282f);
			}
			.mp-search-input {
				width: 100%;
				box-sizing: border-box;
				padding: 5px 8px;
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 4px;
				background: var(--surface-2, #1c232d);
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-mono, monospace);
				font-size: 12px;
			}
			.mp-col-headers {
				display: grid;
				grid-template-columns: 1fr 72px 72px 72px;
				gap: 0 8px;
				padding: 6px 12px;
				border-bottom: 1px solid var(--border-subtle, #23282f);
			}
			.mp-col-header {
				font-size: 10px;
				font-weight: 600;
				text-transform: uppercase;
				letter-spacing: 0.06em;
				color: var(--text-3, #c0c7d0);
			}
			.mp-skel-row {
				display: grid;
				grid-template-columns: 1fr 72px 72px 72px;
				gap: 0 8px;
				padding: 10px 12px;
				border-bottom: 1px solid var(--border-subtle, #23282f);
				align-items: center;
			}
			.mp-skel-cell {
				height: 12px;
				border-radius: 4px;
				background: var(--surface-2, #1c232d);
				animation: mp-pulse 1.4s ease-in-out infinite;
			}
			.mp-skel-cell--wide {
				width: 80%;
			}
			.mp-skel-cell--med {
				width: 60%;
				margin-left: auto;
			}
			@keyframes mp-pulse {
				0%,
				100% {
					opacity: 1;
				}
				50% {
					opacity: 0.35;
				}
			}
		</style>

		<div class="mp-demo">
			<p class="mp-label">Model picker — loading state (spec §4.5):</p>
			<div class="mp-popover">
				<div class="mp-search">
					<input
						type="text"
						class="mp-search-input"
						placeholder="filter by name or provider…"
						disabled
					/>
				</div>
				<div class="mp-col-headers">
					<span class="mp-col-header">Model</span>
					<span class="mp-col-header">Context</span>
					<span class="mp-col-header">$/M in</span>
					<span class="mp-col-header">$/M out</span>
				</div>
				{#each { length: 3 } as _, i (i)}
					<div class="mp-skel-row">
						<span class="mp-skel-cell mp-skel-cell--wide"></span>
						<span class="mp-skel-cell mp-skel-cell--med"></span>
						<span class="mp-skel-cell mp-skel-cell--med"></span>
						<span class="mp-skel-cell mp-skel-cell--med"></span>
					</div>
				{/each}
			</div>
		</div>
	</div>
</Story>

<Story
	name="CalmWarningAnnotation"
	parameters={{
		docs: {
			description: {
				story:
					'Authority=command, mode=drive. Annotation-only treatment: factual option labels, subtle rose dot on the chip. No warning banner, no red border, no ⚠ suffix.'
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
			.cw-demo {
				max-width: 580px;
				margin: 40px auto;
				display: flex;
				flex-direction: column;
				gap: 24px;
			}
			.cw-label {
				font-size: 11px;
				font-weight: 600;
				color: var(--text-3, #c0c7d0);
				text-transform: uppercase;
				letter-spacing: 0.05em;
				margin-bottom: 4px;
			}
			.se-fieldset {
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 6px;
				padding: 14px 16px;
				display: flex;
				flex-direction: column;
				gap: 14px;
			}
			.se-legend {
				font-size: 10px;
				font-weight: 600;
				text-transform: uppercase;
				letter-spacing: 0.06em;
				color: var(--text-3, #c0c7d0);
				padding: 0 4px;
			}
			.se-field {
				display: flex;
				flex-direction: column;
				gap: 4px;
			}
			.se-label {
				font-size: 11px;
				font-weight: 600;
				color: var(--text-2, #adb5be);
				display: flex;
				align-items: center;
				gap: 6px;
			}
			.se-label__help-icon {
				display: inline-flex;
				align-items: center;
				justify-content: center;
				width: 13px;
				height: 13px;
				border-radius: 50%;
				border: 1px solid var(--border-color, #30363d);
				color: var(--text-3, #c0c7d0);
				font-size: 9px;
				font-weight: 700;
				cursor: help;
				flex-shrink: 0;
			}
			.se-select {
				padding: 6px 8px;
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 4px;
				background: var(--surface-2, #1c232d);
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				width: 100%;
				cursor: pointer;
			}
			.chip-row {
				display: flex;
				flex-wrap: wrap;
				gap: 6px;
			}
			.se-chip {
				display: inline-flex;
				align-items: center;
				gap: 4px;
				padding: 2px 8px;
				border: 1px solid;
				border-radius: 4px;
				font-family: var(--font-mono, monospace);
				font-size: 11px;
				font-weight: 600;
				text-transform: uppercase;
			}
			.se-chip--neutral {
				color: var(--text-2, #adb5be);
				background: var(--surface-2, #1c232d);
				border-color: var(--border-subtle, #23282f);
			}
			.se-chip--command {
				color: var(--status-steering-fg);
				background: var(--status-steering-bg);
				border-color: var(--status-steering-border);
			}
			.se-chip--drive {
				color: var(--status-steering-fg);
				background: var(--status-steering-bg);
				border-color: var(--status-steering-border);
			}
			.chip-annotation-dot {
				display: inline-block;
				width: 5px;
				height: 5px;
				border-radius: 50%;
				background: var(--status-steering-fg);
				flex-shrink: 0;
			}
			.cw-comparison {
				display: grid;
				grid-template-columns: 1fr 1fr;
				gap: 20px;
			}
			.cw-col-label {
				font-size: 11px;
				font-weight: 600;
				color: var(--text-3, #c0c7d0);
				text-transform: uppercase;
				margin-bottom: 8px;
			}
			.cw-wrong {
				padding: 12px 14px;
				background: rgba(196, 128, 128, 0.04);
				border: 1px solid rgba(196, 128, 128, 0.2);
				border-radius: 6px;
				font-size: 11px;
				color: var(--text-2, #adb5be);
				line-height: 1.5;
			}
			.cw-right {
				padding: 12px 14px;
				background: rgba(184, 132, 158, 0.04);
				border: 1px solid var(--status-steering-border);
				border-radius: 6px;
				font-size: 11px;
				color: var(--text-2, #adb5be);
				line-height: 1.5;
			}
			.cw-check {
				color: var(--status-success-fg);
				font-weight: 700;
			}
			.cw-cross {
				color: var(--status-danger-fg);
				font-weight: 700;
			}
		</style>

		<div class="cw-demo">
			<div>
				<p class="cw-label">Warning treatment comparison</p>
				<div class="cw-comparison">
					<div>
						<p class="cw-col-label"><span class="cw-cross">✗</span> Old (removed)</p>
						<div class="cw-wrong">
							• Large amber "High-impact mode active" banner<br />
							• ⚠ high-impact suffix in option text<br />
							• Orange/amber border on select element<br />
							• Repeated on every form render<br />
							• Fleet-level danger banner above table
						</div>
					</div>
					<div>
						<p class="cw-col-label"><span class="cw-check">✓</span> New (spec §5)</p>
						<div class="cw-right">
							• Tooltip on label: explains all values at selection time<br />
							• Factual option label: "command — binding directive"<br />
							• Rose dot on chip: subtle permanent ID marker<br />
							• No banner, no border change, no alarm chrome<br />
							• Fleet chips readable without ambient anxiety
						</div>
					</div>
				</div>
			</div>

			<div>
				<p class="cw-label">Abilities fieldset — command + drive selected (correct treatment)</p>
				<fieldset class="se-fieldset">
					<legend class="se-legend">Abilities</legend>

					<div class="se-field">
						<label class="se-label" for="cw-authority">
							Authority
							<span class="se-label__help-icon" aria-hidden="true">?</span>
						</label>
						<select id="cw-authority" class="se-select">
							<option value="supply_context">supply context — adds background info</option>
							<option value="redirect">redirect — suggests a change of direction</option>
							<option value="command" selected>command — binding directive</option>
						</select>
					</div>

					<div class="se-field">
						<label class="se-label" for="cw-mode">
							Mode
							<span class="se-label__help-icon" aria-hidden="true">?</span>
						</label>
						<select id="cw-mode" class="se-select">
							<option value="augment">augment — adds alongside existing context</option>
							<option value="drive" selected>drive — overrides next turn</option>
						</select>
					</div>
				</fieldset>
			</div>

			<div>
				<p class="cw-label">Fleet table chips — annotation dot (spec §5.3)</p>
				<p style="font-size: 12px; color: var(--text-2, #adb5be); margin: 0 0 10px;">
					The rose dot is a permanent identifier for command/drive — it helps operators scan the
					fleet quickly. It is not an alarm; it doesn't change color or pulse.
				</p>
				<div class="chip-row">
					<span class="se-chip se-chip--neutral">supply context</span>
					<span class="se-chip se-chip--neutral">redirect</span>
					<span class="se-chip se-chip--command">
						<span class="chip-annotation-dot" aria-hidden="true"></span>
						command
					</span>

					<span style="color: var(--border-color, #30363d);">·</span>

					<span class="se-chip se-chip--neutral">augment</span>
					<span class="se-chip se-chip--drive">
						<span class="chip-annotation-dot" aria-hidden="true"></span>
						drive
					</span>
				</div>
			</div>
		</div>
	</div>
</Story>

<Story
	name="FleetWithOnboarding"
	parameters={{
		docs: {
			description: {
				story:
					'Fleet tab empty state: Bot icon, heading, 2-sentence explanation of what a steerer is, Create button. No "No steerers registered yet." alone.'
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
			.st-page {
				display: flex;
				flex-direction: column;
				height: 100vh;
			}
			.st-header {
				display: flex;
				align-items: flex-start;
				justify-content: space-between;
				gap: 12px;
				padding: 20px 24px 16px;
				border-bottom: 1px solid var(--border-color, #30363d);
			}
			.st-header__left {
				display: flex;
				flex-direction: column;
				gap: 2px;
			}
			.st-title {
				margin: 0;
				font-size: 1.1rem;
				font-weight: 600;
				color: var(--text-strong, #dde2e8);
			}
			.st-subtitle {
				margin: 0;
				font-size: 12px;
				color: var(--text-3, #c0c7d0);
			}
			.st-btn--primary {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 6px 12px;
				border-radius: 4px;
				font-size: 12px;
				font-family: var(--font-mono, monospace);
				font-weight: 500;
				text-decoration: none;
				border: 1px solid var(--status-steering-border);
				color: var(--status-steering-fg);
				background: var(--status-steering-bg);
			}
			.se-empty-state {
				display: flex;
				flex-direction: column;
				align-items: center;
				justify-content: center;
				gap: 12px;
				padding: 80px 32px;
				text-align: center;
				flex: 1;
			}
			.se-empty-state__icon {
				color: var(--text-3, #c0c7d0);
				font-size: 40px;
				line-height: 1;
				margin-bottom: 4px;
			}
			.se-empty-state__heading {
				margin: 0;
				font-family: var(--font-mono, monospace);
				font-size: 14px;
				font-weight: 600;
				color: var(--text-strong, #dde2e8);
			}
			.se-empty-state__body {
				margin: 0;
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				color: var(--text-2, #adb5be);
				max-width: 360px;
				line-height: 1.6;
			}
			.se-empty-state__action {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 7px 16px;
				border-radius: 4px;
				font-size: 12px;
				font-family: var(--font-mono, monospace);
				font-weight: 500;
				text-decoration: none;
				border: 1px solid var(--status-steering-border);
				color: var(--status-steering-fg);
				background: var(--status-steering-bg);
				margin-top: 4px;
			}
		</style>

		<section class="st-page">
			<header class="st-header">
				<div class="st-header__left">
					<h2 class="st-title">Steerer Fleet</h2>
					<p class="st-subtitle">
						Registered steerers — manage the team, edit prompts, enrol sessions.
					</p>
				</div>
				<div>
					<a href={resolve('steering/new')} class="st-btn--primary"> + New steerer </a>
				</div>
			</header>

			<div class="se-empty-state">
				<div class="se-empty-state__icon" aria-hidden="true">🤖</div>
				<p class="se-empty-state__heading">No steerers registered yet</p>
				<p class="se-empty-state__body">
					A steerer is a lightweight LLM agent that monitors a live session for drift and can inject
					corrective messages. Create one to get started.
				</p>
				<a href={resolve('steering/new')} class="se-empty-state__action"> + New steerer </a>
			</div>
		</section>
	</div>
</Story>

<Story
	name="GuidedStep1SessionPicker"
	parameters={{
		docs: {
			description: {
				story:
					'Guided create flow Step 1: session picker, search, recent session list, "How does steering work?" contextual link. No session selected yet.'
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
			.gf-page {
				display: flex;
				flex-direction: column;
				height: 100vh;
			}
			.gf-header {
				display: flex;
				align-items: center;
				gap: 12px;
				padding: 16px 24px;
				border-bottom: 1px solid var(--border-color, #30363d);
				flex-shrink: 0;
			}
			.gf-back {
				display: flex;
				align-items: center;
				justify-content: center;
				width: 28px;
				height: 28px;
				border-radius: 4px;
				border: 1px solid var(--border-subtle, #23282f);
				color: var(--text-2, #adb5be);
				text-decoration: none;
				font-size: 14px;
				flex-shrink: 0;
			}
			.gf-back:hover {
				border-color: var(--status-steering-border);
				color: var(--status-steering-fg);
			}
			.gf-title {
				margin: 0;
				font-size: 1.05rem;
				font-weight: 600;
				color: var(--text-strong, #dde2e8);
			}
			.gf-body {
				max-width: 640px;
				margin: 0 auto;
				padding: 32px 24px 48px;
				display: flex;
				flex-direction: column;
				gap: 24px;
				flex: 1;
			}
			.gf-steps {
				display: flex;
				align-items: center;
				gap: 0;
				margin-bottom: 4px;
			}
			.gf-step {
				display: flex;
				align-items: center;
				gap: 6px;
				font-size: 11px;
				font-weight: 600;
				color: var(--text-3, #c0c7d0);
			}
			.gf-step--active {
				color: var(--status-steering-fg);
			}
			.gf-step__num {
				display: inline-flex;
				align-items: center;
				justify-content: center;
				width: 20px;
				height: 20px;
				border-radius: 50%;
				border: 1px solid;
				font-size: 10px;
			}
			.gf-step--active .gf-step__num {
				border-color: var(--status-steering-border);
				background: var(--status-steering-bg);
				color: var(--status-steering-fg);
			}
			.gf-step--inactive .gf-step__num {
				border-color: var(--border-subtle, #23282f);
				color: var(--text-3, #c0c7d0);
			}
			.gf-step-sep {
				color: var(--border-color, #30363d);
				margin: 0 10px;
				font-size: 10px;
			}
			.gf-step-heading {
				margin: 0 0 4px;
				font-size: 1.1rem;
				font-weight: 600;
				color: var(--text-strong, #dde2e8);
			}
			.gf-step-sub {
				margin: 0 0 4px;
				font-size: 12px;
				color: var(--text-2, #adb5be);
				line-height: 1.6;
			}
			.gf-guide-link {
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				text-decoration: none;
				display: inline-flex;
				align-items: center;
				gap: 4px;
			}
			.gf-guide-link:hover {
				color: var(--status-steering-fg);
				text-decoration: underline;
			}
			.gf-picker {
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 6px;
				overflow: hidden;
			}
			.gf-picker-search {
				padding: 10px 12px;
				border-bottom: 1px solid var(--border-subtle, #23282f);
			}
			.gf-picker-search-input {
				width: 100%;
				box-sizing: border-box;
				padding: 6px 8px;
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 4px;
				background: var(--surface-2, #1c232d);
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-mono, monospace);
				font-size: 12px;
			}
			.gf-picker-search-input::placeholder {
				color: var(--text-3, #c0c7d0);
			}
			.gf-picker-label {
				padding: 8px 12px 4px;
				font-size: 10px;
				font-weight: 600;
				text-transform: uppercase;
				letter-spacing: 0.06em;
				color: var(--text-3, #c0c7d0);
			}
			.gf-session-row {
				display: flex;
				align-items: center;
				gap: 12px;
				padding: 10px 12px;
				border-bottom: 1px solid var(--border-subtle, #23282f);
				cursor: pointer;
			}
			.gf-session-row:hover {
				background: color-mix(in srgb, var(--surface-2, #1c232d) 60%, transparent 40%);
			}
			.gf-session-row:last-child {
				border-bottom: none;
			}
			.gf-session-id {
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				font-weight: 600;
				color: var(--text-1, #c2c9d1);
				flex-shrink: 0;
			}
			.gf-session-meta {
				display: flex;
				flex-direction: column;
				gap: 1px;
				flex: 1;
				min-width: 0;
			}
			.gf-session-branch {
				font-size: 11px;
				color: var(--text-2, #adb5be);
				overflow: hidden;
				text-overflow: ellipsis;
				white-space: nowrap;
			}
			.gf-session-stats {
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				display: flex;
				gap: 12px;
			}
			.gf-radio {
				display: flex;
				align-items: center;
				justify-content: center;
				width: 14px;
				height: 14px;
				border-radius: 50%;
				border: 1px solid var(--border-color, #30363d);
				flex-shrink: 0;
			}
			.gf-divider {
				display: flex;
				align-items: center;
				gap: 10px;
				margin: 4px 0;
			}
			.gf-divider-line {
				flex: 1;
				height: 1px;
				background: var(--border-subtle, #23282f);
			}
			.gf-divider-text {
				font-size: 10px;
				color: var(--text-3, #c0c7d0);
				white-space: nowrap;
			}
			.gf-manual-input {
				padding: 6px 8px;
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 4px;
				background: var(--surface-2, #1c232d);
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				width: 100%;
				box-sizing: border-box;
			}
			.gf-manual-input::placeholder {
				color: var(--text-3, #c0c7d0);
			}
			.gf-footer {
				display: flex;
				align-items: center;
				justify-content: space-between;
				padding-top: 8px;
				border-top: 1px solid var(--border-subtle, #23282f);
			}
			.gf-btn {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 7px 16px;
				border-radius: 4px;
				font-size: 12px;
				font-family: var(--font-mono, monospace);
				font-weight: 500;
				text-decoration: none;
				border: 1px solid;
				cursor: pointer;
			}
			.gf-btn--primary {
				color: var(--status-steering-fg);
				border-color: var(--status-steering-border);
				background: var(--status-steering-bg);
			}
			.gf-btn--disabled {
				opacity: 0.4;
				cursor: not-allowed;
			}
			.gf-btn--ghost {
				color: var(--text-2, #adb5be);
				border-color: transparent;
				background: transparent;
				font-size: 11px;
			}
			.gf-escape-link {
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				text-decoration: none;
			}
			.gf-escape-link:hover {
				color: var(--text-2, #adb5be);
				text-decoration: underline;
			}
		</style>

		<div class="gf-page">
			<header class="gf-header">
				<a href={resolve('/steering?tab=fleet')} class="gf-back" aria-label="Back to fleet">←</a>
				<h2 class="gf-title">New steerer</h2>
			</header>

			<div class="gf-body">
				<nav class="gf-steps" aria-label="Create flow steps">
					<div class="gf-step gf-step--active">
						<span class="gf-step__num">1</span>
						Pick session
					</div>
					<span class="gf-step-sep">›</span>
					<div class="gf-step gf-step--inactive">
						<span class="gf-step__num">2</span>
						Session context
					</div>
					<span class="gf-step-sep">›</span>
					<div class="gf-step gf-step--inactive">
						<span class="gf-step__num">3</span>
						Define steer
					</div>
					<span class="gf-step-sep">›</span>
					<div class="gf-step gf-step--inactive">
						<span class="gf-step__num">4</span>
						Confirm
					</div>
				</nav>

				<div>
					<h3 class="gf-step-heading">Which session do you want to steer?</h3>
					<p class="gf-step-sub">
						Pick a live session. The steerer will watch this session's turns for drift and can
						inject corrections when it wanders off track.
					</p>
					<a href={resolve('/help/guides/steering-concepts')} class="gf-guide-link">
						How does steering work? ↗
					</a>
				</div>

				<div class="gf-picker" role="listbox" aria-label="Recent sessions">
					<div class="gf-picker-search">
						<input
							type="text"
							class="gf-picker-search-input"
							placeholder="Search sessions…"
							aria-label="Filter sessions"
						/>
					</div>
					<div class="gf-picker-label" aria-hidden="true">
						Recent sessions (live or active today)
					</div>

					<div class="gf-session-row" role="option" aria-selected="false" tabindex="0">
						<span class="gf-radio" aria-hidden="true"></span>
						<div class="gf-session-meta">
							<span class="gf-session-id">64b50581</span>
							<span class="gf-session-branch">feat/phase13-steering-build</span>
						</div>
						<div class="gf-session-stats">
							<span>42 requests</span>
							<span>2 min ago</span>
						</div>
					</div>

					<div class="gf-session-row" role="option" aria-selected="false" tabindex="0">
						<span class="gf-radio" aria-hidden="true"></span>
						<div class="gf-session-meta">
							<span class="gf-session-id">a739eb09</span>
							<span class="gf-session-branch">feat/phase13-steering-build</span>
						</div>
						<div class="gf-session-stats">
							<span>18 requests</span>
							<span>4 min ago</span>
						</div>
					</div>

					<div class="gf-session-row" role="option" aria-selected="false" tabindex="0">
						<span class="gf-radio" aria-hidden="true"></span>
						<div class="gf-session-meta">
							<span class="gf-session-id">9cd8f3ab</span>
							<span class="gf-session-branch">main</span>
						</div>
						<div class="gf-session-stats">
							<span>7 requests</span>
							<span>1 hr ago</span>
						</div>
					</div>
				</div>

				<div>
					<div class="gf-divider" aria-hidden="true">
						<div class="gf-divider-line"></div>
						<span class="gf-divider-text">or enter a session UUID directly</span>
						<div class="gf-divider-line"></div>
					</div>
					<input
						type="text"
						class="gf-manual-input"
						placeholder="e.g. 64b50581-7065-4013-8f77-dd7d8adeddb4"
						aria-label="Session UUID"
					/>
				</div>

				<div class="gf-footer">
					<a href={resolve('steering/new?mode=blank')} class="gf-escape-link">
						or create a blank steerer →
					</a>
					<button
						type="button"
						class="gf-btn gf-btn--primary gf-btn--disabled"
						disabled
						aria-disabled="true"
					>
						Next: See what this session is doing →
					</button>
				</div>
			</div>
		</div>
	</div>
</Story>

<Story
	name="GuidedStep2SessionPreview"
	parameters={{
		docs: {
			description: {
				story:
					'Guided create flow Step 2: session context panel showing last 3 turns (full text, no truncation) and thread anchor if present. Data from GET /api/sessions/{id}/overview.'
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
			.gf-page {
				display: flex;
				flex-direction: column;
				height: 100vh;
			}
			.gf-header {
				display: flex;
				align-items: center;
				gap: 12px;
				padding: 16px 24px;
				border-bottom: 1px solid var(--border-color, #30363d);
				flex-shrink: 0;
			}
			.gf-back {
				display: flex;
				align-items: center;
				justify-content: center;
				width: 28px;
				height: 28px;
				border-radius: 4px;
				border: 1px solid var(--border-subtle, #23282f);
				color: var(--text-2, #adb5be);
				text-decoration: none;
				font-size: 14px;
			}
			.gf-title {
				margin: 0;
				font-size: 1.05rem;
				font-weight: 600;
				color: var(--text-strong, #dde2e8);
			}
			.gf-body {
				max-width: 680px;
				margin: 0 auto;
				padding: 32px 24px 48px;
				display: flex;
				flex-direction: column;
				gap: 24px;
				flex: 1;
				overflow: hidden;
			}
			.gf-steps {
				display: flex;
				align-items: center;
				gap: 0;
			}
			.gf-step {
				display: flex;
				align-items: center;
				gap: 6px;
				font-size: 11px;
				font-weight: 600;
				color: var(--text-3, #c0c7d0);
			}
			.gf-step--active {
				color: var(--status-steering-fg);
			}
			.gf-step--done {
				color: var(--status-success-fg);
			}
			.gf-step__num {
				display: inline-flex;
				align-items: center;
				justify-content: center;
				width: 20px;
				height: 20px;
				border-radius: 50%;
				border: 1px solid;
				font-size: 10px;
			}
			.gf-step--active .gf-step__num {
				border-color: var(--status-steering-border);
				background: var(--status-steering-bg);
				color: var(--status-steering-fg);
			}
			.gf-step--done .gf-step__num {
				border-color: rgba(74, 159, 111, 0.3);
				background: rgba(74, 159, 111, 0.08);
				color: var(--status-success-fg);
			}
			.gf-step--inactive .gf-step__num {
				border-color: var(--border-subtle, #23282f);
				color: var(--text-3, #c0c7d0);
			}
			.gf-step-sep {
				color: var(--border-color, #30363d);
				margin: 0 10px;
				font-size: 10px;
			}
			.gf-step-heading {
				margin: 0 0 4px;
				font-size: 1.1rem;
				font-weight: 600;
				color: var(--text-strong, #dde2e8);
			}
			.gf-step-sub {
				margin: 0;
				font-size: 12px;
				color: var(--text-2, #adb5be);
				line-height: 1.6;
			}
			.gf-anchor-panel {
				display: flex;
				align-items: flex-start;
				gap: 10px;
				padding: 12px 14px;
				background: var(--status-steering-bg);
				border: 1px solid var(--status-steering-border);
				border-radius: 6px;
				flex-shrink: 0;
			}
			.gf-anchor-label {
				font-size: 10px;
				font-weight: 600;
				text-transform: uppercase;
				letter-spacing: 0.06em;
				color: var(--status-steering-fg);
				white-space: nowrap;
				flex-shrink: 0;
			}
			.gf-anchor-text {
				font-size: 12px;
				color: var(--text-1, #c2c9d1);
				line-height: 1.5;
			}
			.gf-turns-area {
				flex: 1;
				overflow-y: auto;
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 6px;
				min-height: 0;
				max-height: 380px;
			}
			.gf-turn {
				padding: 14px 16px;
				border-bottom: 1px solid var(--border-subtle, #23282f);
			}
			.gf-turn:last-child {
				border-bottom: none;
			}
			.gf-turn-meta {
				font-size: 10px;
				color: var(--text-3, #c0c7d0);
				margin-bottom: 8px;
				display: flex;
				align-items: center;
				gap: 8px;
			}
			.gf-turn-divider {
				flex: 1;
				height: 1px;
				background: var(--border-subtle, #23282f);
			}
			.gf-turn-role {
				font-size: 10px;
				font-weight: 600;
				text-transform: uppercase;
				letter-spacing: 0.05em;
				margin-bottom: 3px;
			}
			.gf-turn-role--user {
				color: var(--text-2, #adb5be);
			}
			.gf-turn-role--claude {
				color: var(--text-3, #c0c7d0);
			}
			.gf-turn-text {
				font-size: 12px;
				color: var(--text-1, #c2c9d1);
				line-height: 1.5;
				white-space: pre-wrap;
				word-break: break-word;
			}
			.gf-turn-pair {
				display: flex;
				flex-direction: column;
				gap: 10px;
			}
			.gf-footer {
				display: flex;
				align-items: center;
				justify-content: space-between;
				padding-top: 8px;
				border-top: 1px solid var(--border-subtle, #23282f);
				flex-shrink: 0;
			}
			.gf-btn {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 7px 16px;
				border-radius: 4px;
				font-size: 12px;
				font-family: var(--font-mono, monospace);
				font-weight: 500;
				text-decoration: none;
				border: 1px solid;
				cursor: pointer;
			}
			.gf-btn--primary {
				color: var(--status-steering-fg);
				border-color: var(--status-steering-border);
				background: var(--status-steering-bg);
			}
			.gf-btn--ghost {
				color: var(--text-2, #adb5be);
				border-color: var(--border-subtle, #23282f);
				background: transparent;
			}
		</style>

		<div class="gf-page">
			<header class="gf-header">
				<a href={resolve('/steering?tab=fleet')} class="gf-back" aria-label="Back">←</a>
				<h2 class="gf-title">New steerer</h2>
			</header>

			<div class="gf-body">
				<nav class="gf-steps" aria-label="Create flow steps">
					<div class="gf-step gf-step--done"><span class="gf-step__num">✓</span> Pick session</div>
					<span class="gf-step-sep">›</span>
					<div class="gf-step gf-step--active">
						<span class="gf-step__num">2</span> Session context
					</div>
					<span class="gf-step-sep">›</span>
					<div class="gf-step gf-step--inactive">
						<span class="gf-step__num">3</span> Define steer
					</div>
					<span class="gf-step-sep">›</span>
					<div class="gf-step gf-step--inactive"><span class="gf-step__num">4</span> Confirm</div>
				</nav>

				<div>
					<h3 class="gf-step-heading">What is session 64b50581 doing?</h3>
					<p class="gf-step-sub">
						Here are the last few turns. Use this to decide what you want your steerer to watch for.
					</p>
				</div>

				<div class="gf-anchor-panel" role="note" aria-label="Thread anchor">
					<span class="gf-anchor-label">Thread anchor</span>
					<p class="gf-anchor-text">
						"Implement the steering backend — see the steering roadmap issue. Do not refactor
						unrelated files."
					</p>
				</div>

				<div class="gf-turns-area" aria-label="Recent session turns">
					<div class="gf-turn">
						<div class="gf-turn-meta">
							Turn 14 <span class="gf-turn-divider"></span> 2 min ago
						</div>
						<div class="gf-turn-pair">
							<div>
								<div class="gf-turn-role gf-turn-role--user">User</div>
								<p class="gf-turn-text">
									Fix the failing migration test in tests/integration/db_test.rs
								</p>
							</div>
							<div>
								<div class="gf-turn-role gf-turn-role--claude">Claude</div>
								<p class="gf-turn-text">
									I found the issue — the test uses the dev DB directly instead of the ephemeral
									per-test database. I'll fix the TestDb::acquire call to use the correct
									MIKE_TEST_ADMIN_URL pattern.
								</p>
							</div>
						</div>
					</div>

					<div class="gf-turn">
						<div class="gf-turn-meta">
							Turn 13 <span class="gf-turn-divider"></span> 8 min ago
						</div>
						<div class="gf-turn-pair">
							<div>
								<div class="gf-turn-role gf-turn-role--user">User</div>
								<p class="gf-turn-text">Run make test-db and tell me what's failing</p>
							</div>
							<div>
								<div class="gf-turn-role gf-turn-role--claude">Claude</div>
								<p class="gf-turn-text">
									Running make test-db... 3 failures in the persistence/ tests. All related to the
									new `steered_requests_v` view — the view is missing from the test template DB
									because the last migration wasn't applied there.
								</p>
							</div>
						</div>
					</div>

					<div class="gf-turn">
						<div class="gf-turn-meta">
							Turn 12 <span class="gf-turn-divider"></span> 14 min ago
						</div>
						<div class="gf-turn-pair">
							<div>
								<div class="gf-turn-role gf-turn-role--user">User</div>
								<p class="gf-turn-text">What's left before we can merge Phase 13?</p>
							</div>
							<div>
								<div class="gf-turn-role gf-turn-role--claude">Claude</div>
								<p class="gf-turn-text">
									The backend gates are [1]-[5] shipped. Gate [6] (REST routes + SteeringFired event
									+ frontend) is done. Outstanding: (1) fix db_test.rs failures, (2) docs thread
									checkpoint, (3) PR review.
								</p>
							</div>
						</div>
					</div>
				</div>

				<div class="gf-footer">
					<button type="button" class="gf-btn gf-btn--ghost">← Back</button>
					<button type="button" class="gf-btn gf-btn--primary"> Next: Define the steer → </button>
				</div>
			</div>
		</div>
	</div>
</Story>

<Story
	name="GuidedStep3SteerCraft"
	parameters={{
		docs: {
			description: {
				story:
					'Guided create flow Step 3: intent textarea + inline authority and mode selection with one-line explanations. supply_context + augment selected by default.'
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
			.gf-page {
				display: flex;
				flex-direction: column;
				height: 100vh;
			}
			.gf-header {
				display: flex;
				align-items: center;
				gap: 12px;
				padding: 16px 24px;
				border-bottom: 1px solid var(--border-color, #30363d);
				flex-shrink: 0;
			}
			.gf-back {
				display: flex;
				align-items: center;
				justify-content: center;
				width: 28px;
				height: 28px;
				border-radius: 4px;
				border: 1px solid var(--border-subtle, #23282f);
				color: var(--text-2, #adb5be);
				text-decoration: none;
				font-size: 14px;
			}
			.gf-title {
				margin: 0;
				font-size: 1.05rem;
				font-weight: 600;
				color: var(--text-strong, #dde2e8);
			}
			.gf-body {
				max-width: 640px;
				margin: 0 auto;
				padding: 32px 24px 48px;
				display: flex;
				flex-direction: column;
				gap: 24px;
				flex: 1;
				overflow-y: auto;
			}
			.gf-steps {
				display: flex;
				align-items: center;
				gap: 0;
			}
			.gf-step {
				display: flex;
				align-items: center;
				gap: 6px;
				font-size: 11px;
				font-weight: 600;
				color: var(--text-3, #c0c7d0);
			}
			.gf-step--active {
				color: var(--status-steering-fg);
			}
			.gf-step--done {
				color: var(--status-success-fg);
			}
			.gf-step__num {
				display: inline-flex;
				align-items: center;
				justify-content: center;
				width: 20px;
				height: 20px;
				border-radius: 50%;
				border: 1px solid;
				font-size: 10px;
			}
			.gf-step--active .gf-step__num {
				border-color: var(--status-steering-border);
				background: var(--status-steering-bg);
				color: var(--status-steering-fg);
			}
			.gf-step--done .gf-step__num {
				border-color: rgba(74, 159, 111, 0.3);
				background: rgba(74, 159, 111, 0.08);
				color: var(--status-success-fg);
			}
			.gf-step--inactive .gf-step__num {
				border-color: var(--border-subtle, #23282f);
				color: var(--text-3, #c0c7d0);
			}
			.gf-step-sep {
				color: var(--border-color, #30363d);
				margin: 0 10px;
				font-size: 10px;
			}
			.gf-step-heading {
				margin: 0 0 4px;
				font-size: 1.1rem;
				font-weight: 600;
				color: var(--text-strong, #dde2e8);
			}
			.gf-step-sub {
				margin: 0;
				font-size: 12px;
				color: var(--text-2, #adb5be);
				line-height: 1.6;
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
			.gf-label__hint {
				font-weight: 400;
				color: var(--text-3, #c0c7d0);
			}
			.gf-textarea {
				padding: 10px 12px;
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 4px;
				background: var(--surface-2, #1c232d);
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				line-height: 1.6;
				width: 100%;
				box-sizing: border-box;
				resize: vertical;
				min-height: 80px;
			}
			.gf-live-preview {
				padding: 8px 10px;
				background: var(--surface-2, #1c232d);
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 4px;
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				line-height: 1.5;
			}
			.gf-live-preview strong {
				color: var(--text-2, #adb5be);
			}
			.gf-section-label {
				font-size: 11px;
				font-weight: 600;
				color: var(--text-2, #adb5be);
				margin-bottom: 2px;
			}
			.gf-radio-group {
				display: flex;
				flex-direction: column;
				gap: 2px;
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 6px;
				overflow: hidden;
			}
			.gf-radio-option {
				display: flex;
				align-items: flex-start;
				gap: 12px;
				padding: 11px 14px;
				cursor: pointer;
				border-bottom: 1px solid var(--border-subtle, #23282f);
			}
			.gf-radio-option:last-child {
				border-bottom: none;
			}
			.gf-radio-option:hover {
				background: color-mix(in srgb, var(--surface-2, #1c232d) 60%, transparent 40%);
			}
			.gf-radio-option--selected {
				background: var(--status-steering-bg);
			}
			.gf-radio-btn {
				display: flex;
				align-items: center;
				justify-content: center;
				width: 14px;
				height: 14px;
				border-radius: 50%;
				border: 1px solid var(--border-color, #30363d);
				flex-shrink: 0;
				margin-top: 1px;
			}
			.gf-radio-btn--selected {
				border-color: var(--status-steering-border);
				background: var(--status-steering-bg);
			}
			.gf-radio-btn--selected::after {
				content: '';
				display: block;
				width: 6px;
				height: 6px;
				border-radius: 50%;
				background: var(--status-steering-fg);
			}
			.gf-radio-content {
				flex: 1;
				min-width: 0;
			}
			.gf-radio-title {
				font-size: 12px;
				font-weight: 600;
				color: var(--text-1, #c2c9d1);
				margin-bottom: 2px;
			}
			.gf-radio-option--selected .gf-radio-title {
				color: var(--status-steering-fg);
			}
			.gf-radio-desc {
				font-size: 11px;
				color: var(--text-2, #adb5be);
				line-height: 1.4;
			}
			.gf-radio-best {
				font-size: 10px;
				color: var(--text-3, #c0c7d0);
				margin-top: 2px;
			}
			.gf-authority-note {
				padding: 8px 10px;
				background: rgba(196, 144, 96, 0.06);
				border: 1px solid rgba(196, 144, 96, 0.18);
				border-radius: 4px;
				font-size: 11px;
				color: rgba(196, 144, 96, 0.9);
				line-height: 1.5;
				display: none;
			}
			.gf-guide-link {
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				text-decoration: none;
				display: inline-flex;
				align-items: center;
				gap: 4px;
			}
			.gf-guide-link:hover {
				color: var(--status-steering-fg);
				text-decoration: underline;
			}
			.gf-footer {
				display: flex;
				align-items: center;
				justify-content: space-between;
				padding-top: 8px;
				border-top: 1px solid var(--border-subtle, #23282f);
				flex-shrink: 0;
			}
			.gf-btn {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 7px 16px;
				border-radius: 4px;
				font-size: 12px;
				font-family: var(--font-mono, monospace);
				font-weight: 500;
				text-decoration: none;
				border: 1px solid;
				cursor: pointer;
			}
			.gf-btn--primary {
				color: var(--status-steering-fg);
				border-color: var(--status-steering-border);
				background: var(--status-steering-bg);
			}
			.gf-btn--ghost {
				color: var(--text-2, #adb5be);
				border-color: var(--border-subtle, #23282f);
				background: transparent;
			}
		</style>

		<div class="gf-page">
			<header class="gf-header">
				<a href={resolve('/steering?tab=fleet')} class="gf-back" aria-label="Back">←</a>
				<h2 class="gf-title">New steerer</h2>
			</header>

			<div class="gf-body">
				<nav class="gf-steps" aria-label="Create flow steps">
					<div class="gf-step gf-step--done"><span class="gf-step__num">✓</span> Pick session</div>
					<span class="gf-step-sep">›</span>
					<div class="gf-step gf-step--done">
						<span class="gf-step__num">✓</span> Session context
					</div>
					<span class="gf-step-sep">›</span>
					<div class="gf-step gf-step--active">
						<span class="gf-step__num">3</span> Define steer
					</div>
					<span class="gf-step-sep">›</span>
					<div class="gf-step gf-step--inactive"><span class="gf-step__num">4</span> Confirm</div>
				</nav>

				<div>
					<h3 class="gf-step-heading">What should the steerer watch for?</h3>
					<p class="gf-step-sub">
						Describe in plain language what this session should be focused on. The steerer uses this
						as its thread anchor — the reference point against which it evaluates each new turn.
					</p>
				</div>

				<div class="gf-field">
					<label class="gf-label" for="sc-intent">
						What should this session be focused on?
						<span class="gf-label__hint"> — be specific, one or two sentences</span>
					</label>
					<textarea
						id="sc-intent"
						class="gf-textarea"
						rows={4}
						placeholder="Example: 'Implement the steering backend. Do not refactor unrelated files.'"
						>Implement the Phase 13 steering backend. Stay within crates/mike-features/src/steerer/. Do not touch frontend or migration files.</textarea
					>
					<div class="gf-live-preview" aria-live="polite">
						<strong>This becomes the steerer's prompt anchor.</strong> When the session's turns diverge
						from this description, the steerer fires.
					</div>
				</div>

				<div class="gf-field">
					<p class="gf-section-label">How forceful should corrections be?</p>
					<div class="gf-radio-group" role="radiogroup" aria-labelledby="sc-authority-label">
						<label class="gf-radio-option gf-radio-option--selected">
							<span class="gf-radio-btn gf-radio-btn--selected" aria-hidden="true"></span>
							<div class="gf-radio-content">
								<div class="gf-radio-title">Supply context</div>
								<div class="gf-radio-desc">
									Adds background info. Claude may use it or ignore it.
								</div>
								<div class="gf-radio-best">
									Best for: gentle nudges, supplying forgotten context.
								</div>
							</div>
							<input type="radio" name="authority" value="supply_context" checked class="sr-only" />
						</label>
						<label class="gf-radio-option">
							<span class="gf-radio-btn" aria-hidden="true"></span>
							<div class="gf-radio-content">
								<div class="gf-radio-title">Redirect</div>
								<div class="gf-radio-desc">
									Suggests a change of direction. Claude can push back.
								</div>
								<div class="gf-radio-best">
									Best for: catching mild drift, reorienting without interrupting.
								</div>
							</div>
							<input type="radio" name="authority" value="redirect" class="sr-only" />
						</label>
						<label class="gf-radio-option">
							<span class="gf-radio-btn" aria-hidden="true"></span>
							<div class="gf-radio-content">
								<div class="gf-radio-title">Command</div>
								<div class="gf-radio-desc">Binding directive. Treated as operator instruction.</div>
								<div class="gf-radio-best">
									Best for: well-tested steerers, critical tasks, hard stops.
								</div>
							</div>
							<input type="radio" name="authority" value="command" class="sr-only" />
						</label>
					</div>
					<a href={resolve('/help/guides/steering-concepts#authority')} class="gf-guide-link">
						Learn more about authority levels ↗
					</a>
				</div>

				<div class="gf-field">
					<p class="gf-section-label">How should the correction arrive?</p>
					<div class="gf-radio-group" role="radiogroup">
						<label class="gf-radio-option gf-radio-option--selected">
							<span class="gf-radio-btn gf-radio-btn--selected" aria-hidden="true"></span>
							<div class="gf-radio-content">
								<div class="gf-radio-title">
									Augment <span
										style="font-size: 10px; font-weight: 400; color: var(--status-success-fg);"
										>Recommended</span
									>
								</div>
								<div class="gf-radio-desc">
									Adds the correction alongside the session's existing context. Claude generates the
									next turn naturally, with your correction in context.
								</div>
							</div>
							<input type="radio" name="mode" value="augment" checked class="sr-only" />
						</label>
						<label class="gf-radio-option">
							<span class="gf-radio-btn" aria-hidden="true"></span>
							<div class="gf-radio-content">
								<div class="gf-radio-title">Drive</div>
								<div class="gf-radio-desc">
									The steerer's message replaces the session's next turn entirely. Use only when
									augment has repeatedly failed to recapture the session.
								</div>
							</div>
							<input type="radio" name="mode" value="drive" class="sr-only" />
						</label>
					</div>
				</div>

				<div class="gf-footer">
					<button type="button" class="gf-btn gf-btn--ghost">← Back</button>
					<button type="button" class="gf-btn gf-btn--primary">Review + create →</button>
				</div>
			</div>
		</div>
	</div>
</Story>

<Story
	name="GuidedStep3AiAssist"
	parameters={{
		docs: {
			description: {
				story:
					'Step 3 AI-assist layer: LEFT = idle state with "Help me write this →" button. RIGHT = suggestion-with-rationale panel (frozen mock) showing model provenance, Accept/Edit controls, per-field rationale. Manual fields always available. Per spec §12.9.'
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
			.ai-demo {
				display: grid;
				grid-template-columns: 1fr 1fr;
				gap: 24px;
				padding: 28px 24px;
				max-width: 1100px;
				margin: 0 auto;
				align-items: start;
			}
			.ai-col {
				display: flex;
				flex-direction: column;
				gap: 16px;
			}
			.ai-col-label {
				font-size: 10px;
				font-weight: 600;
				text-transform: uppercase;
				letter-spacing: 0.06em;
				color: var(--text-3, #c0c7d0);
				margin-bottom: 4px;
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
			.gf-label__hint {
				font-weight: 400;
				color: var(--text-3, #c0c7d0);
			}
			.gf-textarea {
				padding: 10px 12px;
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 4px;
				background: var(--surface-2, #1c232d);
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				line-height: 1.6;
				width: 100%;
				box-sizing: border-box;
				resize: vertical;
				min-height: 72px;
			}
			.gf-live-preview {
				padding: 8px 10px;
				background: var(--surface-2, #1c232d);
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 4px;
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				line-height: 1.5;
			}
			.gf-live-preview strong {
				color: var(--text-2, #adb5be);
			}
			.gf-btn-assist {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 6px 12px;
				border-radius: 4px;
				font-size: 11px;
				font-family: var(--font-mono, monospace);
				font-weight: 500;
				border: 1px solid var(--border-color, #30363d);
				color: var(--text-2, #adb5be);
				background: var(--surface-2, #1c232d);
				cursor: pointer;
				align-self: flex-start;
			}
			.gf-btn-assist:hover {
				border-color: var(--border-subtle, #23282f);
				color: var(--text-1, #c2c9d1);
			}
			.ai-panel {
				border: 1px solid var(--border-color, #30363d);
				border-radius: 6px;
				overflow: hidden;
			}
			.ai-panel-head {
				display: flex;
				align-items: center;
				justify-content: space-between;
				padding: 9px 12px;
				background: var(--surface-2, #1c232d);
				border-bottom: 1px solid var(--border-subtle, #23282f);
			}
			.ai-panel-title {
				font-size: 10px;
				font-weight: 600;
				text-transform: uppercase;
				letter-spacing: 0.06em;
				color: var(--text-3, #c0c7d0);
				display: flex;
				align-items: center;
				gap: 6px;
			}
			.ai-model-badge {
				display: inline-flex;
				align-items: center;
				padding: 2px 6px;
				background: var(--surface, #161b22);
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 3px;
				font-size: 10px;
				font-weight: 500;
				color: var(--text-3, #c0c7d0);
				text-transform: none;
				letter-spacing: 0;
			}
			.ai-dismiss {
				background: none;
				border: none;
				color: var(--text-3, #c0c7d0);
				cursor: pointer;
				font-size: 14px;
				line-height: 1;
				padding: 2px 4px;
				border-radius: 3px;
			}
			.ai-dismiss:hover {
				color: var(--text-1, #c2c9d1);
				background: var(--surface, #161b22);
			}
			.ai-panel-body {
				display: flex;
				flex-direction: column;
			}
			.ai-row {
				padding: 10px 12px;
				border-bottom: 1px solid var(--border-subtle, #23282f);
			}
			.ai-row:last-child {
				border-bottom: none;
			}
			.ai-row-field {
				font-size: 10px;
				font-weight: 600;
				text-transform: uppercase;
				letter-spacing: 0.06em;
				color: var(--text-3, #c0c7d0);
				margin-bottom: 4px;
			}
			.ai-row-value {
				font-size: 12px;
				color: var(--text-1, #c2c9d1);
				margin-bottom: 4px;
				line-height: 1.5;
			}
			.ai-row-rationale {
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				line-height: 1.4;
				padding-left: 10px;
				border-left: 2px solid var(--border-subtle, #23282f);
			}
			.ai-row-rationale::before {
				content: '↳ ';
			}
			.ai-panel-footer {
				display: flex;
				align-items: center;
				gap: 8px;
				padding: 10px 12px;
				background: var(--surface-2, #1c232d);
				border-top: 1px solid var(--border-subtle, #23282f);
			}
			.gf-btn {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 6px 12px;
				border-radius: 4px;
				font-size: 11px;
				font-family: var(--font-mono, monospace);
				font-weight: 500;
				border: 1px solid;
				cursor: pointer;
			}
			.gf-btn--accept {
				color: var(--status-steering-fg);
				border-color: var(--status-steering-border);
				background: var(--status-steering-bg);
			}
			.gf-btn--ghost {
				color: var(--text-2, #adb5be);
				border-color: var(--border-subtle, #23282f);
				background: transparent;
			}
			.gf-section-label {
				font-size: 11px;
				font-weight: 600;
				color: var(--text-2, #adb5be);
			}
			.gf-radio-group {
				display: flex;
				flex-direction: column;
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 6px;
				overflow: hidden;
			}
			.gf-radio-option {
				display: flex;
				align-items: flex-start;
				gap: 10px;
				padding: 8px 12px;
				cursor: pointer;
				border-bottom: 1px solid var(--border-subtle, #23282f);
			}
			.gf-radio-option:last-child {
				border-bottom: none;
			}
			.gf-radio-option--selected {
				background: var(--status-steering-bg);
			}
			.gf-radio-btn {
				display: flex;
				align-items: center;
				justify-content: center;
				width: 13px;
				height: 13px;
				border-radius: 50%;
				border: 1px solid var(--border-color, #30363d);
				flex-shrink: 0;
				margin-top: 1px;
			}
			.gf-radio-btn--selected {
				border-color: var(--status-steering-border);
				background: var(--status-steering-bg);
			}
			.gf-radio-btn--selected::after {
				content: '';
				display: block;
				width: 5px;
				height: 5px;
				border-radius: 50%;
				background: var(--status-steering-fg);
			}
			.gf-radio-content {
				flex: 1;
				min-width: 0;
			}
			.gf-radio-title {
				font-size: 11px;
				font-weight: 600;
				color: var(--text-1, #c2c9d1);
				margin-bottom: 1px;
			}
			.gf-radio-option--selected .gf-radio-title {
				color: var(--status-steering-fg);
			}
			.gf-radio-desc {
				font-size: 10px;
				color: var(--text-2, #adb5be);
				line-height: 1.3;
			}
			.gf-radio-suggested {
				font-size: 10px;
				font-weight: 400;
				color: var(--text-3, #c0c7d0);
				margin-left: 4px;
			}
		</style>

		<div class="ai-demo">
			<div class="ai-col">
				<p class="ai-col-label">State A — idle (button visible, not yet triggered)</p>

				<div class="gf-field">
					<label class="gf-label" for="ai-intent-idle">
						What should this session be focused on?
						<span class="gf-label__hint"> — be specific</span>
					</label>
					<textarea id="ai-intent-idle" class="gf-textarea" rows={3}
						>Implement Phase 13. Do not refactor unrelated files.</textarea
					>
					<div class="gf-live-preview">
						<strong>This becomes the steerer's prompt anchor.</strong> When the session's turns diverge
						from this, the steerer fires.
					</div>
					<button type="button" class="gf-btn-assist"> Help me write this → </button>
				</div>

				<div class="gf-field">
					<p class="gf-section-label">How forceful should corrections be?</p>
					<div class="gf-radio-group" role="radiogroup">
						<label class="gf-radio-option gf-radio-option--selected">
							<span class="gf-radio-btn gf-radio-btn--selected" aria-hidden="true"></span>
							<div class="gf-radio-content">
								<div class="gf-radio-title">Supply context</div>
								<div class="gf-radio-desc">
									Adds background info. Claude may use it or ignore it.
								</div>
							</div>
							<input type="radio" name="auth-idle" value="supply_context" checked class="sr-only" />
						</label>
						<label class="gf-radio-option">
							<span class="gf-radio-btn" aria-hidden="true"></span>
							<div class="gf-radio-content">
								<div class="gf-radio-title">Redirect</div>
								<div class="gf-radio-desc">Suggests a change. Claude can push back.</div>
							</div>
							<input type="radio" name="auth-idle" value="redirect" class="sr-only" />
						</label>
						<label class="gf-radio-option">
							<span class="gf-radio-btn" aria-hidden="true"></span>
							<div class="gf-radio-content">
								<div class="gf-radio-title">Command</div>
								<div class="gf-radio-desc">Binding directive. Operator instruction.</div>
							</div>
							<input type="radio" name="auth-idle" value="command" class="sr-only" />
						</label>
					</div>
				</div>
			</div>

			<div class="ai-col">
				<p class="ai-col-label">State B — suggestion received (frozen mock, spec §12.9.4)</p>

				<div class="gf-field">
					<label class="gf-label" for="ai-intent-sugg">
						What should this session be focused on?
						<span class="gf-label__hint"> — be specific</span>
					</label>
					<textarea id="ai-intent-sugg" class="gf-textarea" rows={3}
						>Implement Phase 13. Do not refactor unrelated files.</textarea
					>
				</div>

				<div class="ai-panel">
					<div class="ai-panel-head">
						<div class="ai-panel-title">
							Suggestion
							<span class="ai-model-badge">openai/gpt-5.6-luna</span>
						</div>
						<button type="button" class="ai-dismiss" aria-label="Dismiss suggestion">✕</button>
					</div>

					<div class="ai-panel-body">
						<div class="ai-row">
							<div class="ai-row-field">Prompt anchor</div>
							<div class="ai-row-value">
								Stay within crates/mike-features/src/steerer/ and implement the Phase 13 steering fleet
								backend. Do not touch frontend/ or migration files.
							</div>
							<div class="ai-row-rationale">
								Combines your intent with the session's observed focus on steerer/. Narrows scope so
								drift detection fires precisely.
							</div>
						</div>
						<div class="ai-row">
							<div class="ai-row-field">Authority</div>
							<div class="ai-row-value">supply context</div>
							<div class="ai-row-rationale">
								The session is working well; a soft context injection is appropriate before
								escalating to redirect or command.
							</div>
						</div>
						<div class="ai-row">
							<div class="ai-row-field">Mode</div>
							<div class="ai-row-value">augment</div>
							<div class="ai-row-rationale">
								Session is generating turns normally; no need to override yet.
							</div>
						</div>
						<div class="ai-row">
							<div class="ai-row-field">Confidence threshold</div>
							<div class="ai-row-value">0.65</div>
							<div class="ai-row-rationale">
								Slightly looser than 0.7 — the session's scope is narrow, so small drift should
								trigger.
							</div>
						</div>
						<div class="ai-row">
							<div class="ai-row-field">Debounce</div>
							<div class="ai-row-value">3 turns</div>
							<div class="ai-row-rationale">
								Gives the session room to process each steer before the next evaluation.
							</div>
						</div>
					</div>

					<div class="ai-panel-footer">
						<button type="button" class="gf-btn gf-btn--accept">Accept suggestion</button>
						<button type="button" class="gf-btn gf-btn--ghost">Edit manually</button>
					</div>
				</div>

				<div class="gf-field">
					<p class="gf-section-label">How forceful should corrections be?</p>
					<div class="gf-radio-group" role="radiogroup">
						<label class="gf-radio-option gf-radio-option--selected">
							<span class="gf-radio-btn gf-radio-btn--selected" aria-hidden="true"></span>
							<div class="gf-radio-content">
								<div class="gf-radio-title">
									Supply context <span class="gf-radio-suggested">(suggested)</span>
								</div>
								<div class="gf-radio-desc">
									Adds background info. Claude may use it or ignore it.
								</div>
							</div>
							<input type="radio" name="auth-sugg" value="supply_context" checked class="sr-only" />
						</label>
						<label class="gf-radio-option">
							<span class="gf-radio-btn" aria-hidden="true"></span>
							<div class="gf-radio-content">
								<div class="gf-radio-title">Redirect</div>
								<div class="gf-radio-desc">Suggests a change. Claude can push back.</div>
							</div>
							<input type="radio" name="auth-sugg" value="redirect" class="sr-only" />
						</label>
						<label class="gf-radio-option">
							<span class="gf-radio-btn" aria-hidden="true"></span>
							<div class="gf-radio-content">
								<div class="gf-radio-title">Command</div>
								<div class="gf-radio-desc">Binding directive. Operator instruction.</div>
							</div>
							<input type="radio" name="auth-sugg" value="command" class="sr-only" />
						</label>
					</div>
				</div>
			</div>
		</div>
	</div>
</Story>

<Story
	name="GuidedStep4Confirm"
	parameters={{
		docs: {
			description: {
				story:
					'Guided create flow Step 4: read-only summary card of steerer config, Create + enrol button (rose), "What happens next" explanation. Per spec §12.6.'
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
			.gf-page {
				display: flex;
				flex-direction: column;
				height: 100vh;
			}
			.gf-header {
				display: flex;
				align-items: center;
				gap: 12px;
				padding: 16px 24px;
				border-bottom: 1px solid var(--border-color, #30363d);
				flex-shrink: 0;
			}
			.gf-back {
				display: flex;
				align-items: center;
				justify-content: center;
				width: 28px;
				height: 28px;
				border-radius: 4px;
				border: 1px solid var(--border-subtle, #23282f);
				color: var(--text-2, #adb5be);
				text-decoration: none;
				font-size: 14px;
			}
			.gf-title {
				margin: 0;
				font-size: 1.05rem;
				font-weight: 600;
				color: var(--text-strong, #dde2e8);
			}
			.gf-body {
				max-width: 640px;
				margin: 0 auto;
				padding: 32px 24px 48px;
				display: flex;
				flex-direction: column;
				gap: 24px;
				flex: 1;
				overflow-y: auto;
			}
			.gf-steps {
				display: flex;
				align-items: center;
				gap: 0;
			}
			.gf-step {
				display: flex;
				align-items: center;
				gap: 6px;
				font-size: 11px;
				font-weight: 600;
				color: var(--text-3, #c0c7d0);
			}
			.gf-step--active {
				color: var(--status-steering-fg);
			}
			.gf-step--done {
				color: var(--status-success-fg);
			}
			.gf-step__num {
				display: inline-flex;
				align-items: center;
				justify-content: center;
				width: 20px;
				height: 20px;
				border-radius: 50%;
				border: 1px solid;
				font-size: 10px;
			}
			.gf-step--active .gf-step__num {
				border-color: var(--status-steering-border);
				background: var(--status-steering-bg);
				color: var(--status-steering-fg);
			}
			.gf-step--done .gf-step__num {
				border-color: rgba(74, 159, 111, 0.3);
				background: rgba(74, 159, 111, 0.08);
				color: var(--status-success-fg);
			}
			.gf-step-sep {
				color: var(--border-color, #30363d);
				margin: 0 10px;
				font-size: 10px;
			}
			.gf-step-heading {
				margin: 0 0 4px;
				font-size: 1.1rem;
				font-weight: 600;
				color: var(--text-strong, #dde2e8);
			}
			.gf-step-sub {
				margin: 0;
				font-size: 12px;
				color: var(--text-2, #adb5be);
				line-height: 1.6;
			}
			.gf-summary {
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 6px;
				overflow: hidden;
			}
			.gf-summary-head {
				display: flex;
				align-items: center;
				justify-content: space-between;
				padding: 10px 14px;
				background: var(--surface-2, #1c232d);
				border-bottom: 1px solid var(--border-subtle, #23282f);
			}
			.gf-summary-title {
				font-size: 11px;
				font-weight: 600;
				color: var(--text-3, #c0c7d0);
				text-transform: uppercase;
				letter-spacing: 0.06em;
			}
			.gf-edit-link {
				font-size: 11px;
				color: var(--status-steering-fg);
				text-decoration: none;
				background: none;
				border: none;
				padding: 0;
				font-family: inherit;
				cursor: pointer;
			}
			.gf-edit-link:hover {
				text-decoration: underline;
			}
			.gf-summary-rows {
				display: flex;
				flex-direction: column;
			}
			.gf-summary-row {
				display: grid;
				grid-template-columns: 140px 1fr;
				gap: 0 16px;
				padding: 8px 14px;
				border-bottom: 1px solid var(--border-subtle, #23282f);
			}
			.gf-summary-row:last-child {
				border-bottom: none;
			}
			.gf-summary-key {
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
			}
			.gf-summary-val {
				font-size: 12px;
				color: var(--text-1, #c2c9d1);
				word-break: break-word;
			}
			.gf-summary-val--prompt {
				font-size: 11px;
				line-height: 1.5;
				color: var(--text-2, #adb5be);
			}
			.gf-enrol-target {
				display: flex;
				align-items: center;
				gap: 10px;
				padding: 12px 14px;
				background: var(--status-steering-bg);
				border: 1px solid var(--status-steering-border);
				border-radius: 6px;
			}
			.gf-enrol-icon {
				font-size: 18px;
				flex-shrink: 0;
			}
			.gf-enrol-text {
				font-size: 12px;
				color: var(--text-1, #c2c9d1);
				line-height: 1.5;
			}
			.gf-enrol-session-id {
				color: var(--status-steering-fg);
				font-weight: 600;
			}
			.gf-btn-create {
				width: 100%;
				display: flex;
				align-items: center;
				justify-content: center;
				gap: 8px;
				padding: 11px 24px;
				border-radius: 6px;
				font-size: 14px;
				font-family: var(--font-mono, monospace);
				font-weight: 600;
				border: 1px solid var(--status-steering-border);
				color: var(--status-steering-fg);
				background: var(--status-steering-bg);
				cursor: pointer;
			}
			.gf-btn-create:hover {
				background: color-mix(
					in srgb,
					var(--status-steering-bg) 50%,
					var(--status-steering-fg) 10%
				);
			}
			.gf-whats-next {
				padding: 14px 16px;
				background: var(--surface-2, #1c232d);
				border: 1px solid var(--border-subtle, #23282f);
				border-radius: 6px;
			}
			.gf-whats-next__title {
				font-size: 11px;
				font-weight: 600;
				color: var(--text-2, #adb5be);
				margin-bottom: 10px;
				text-transform: uppercase;
				letter-spacing: 0.05em;
			}
			.gf-whats-next__steps {
				margin: 0;
				padding: 0 0 0 16px;
				list-style: decimal;
			}
			.gf-whats-next__steps li {
				font-size: 12px;
				color: var(--text-2, #adb5be);
				line-height: 1.6;
				margin-bottom: 4px;
			}
			.gf-whats-next__steps li:last-child {
				margin-bottom: 0;
			}
			.gf-whats-next__note {
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				margin-top: 10px;
				line-height: 1.5;
			}
			.gf-footer {
				display: flex;
				align-items: center;
				padding-top: 8px;
			}
			.gf-btn {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 7px 16px;
				border-radius: 4px;
				font-size: 12px;
				font-family: var(--font-mono, monospace);
				font-weight: 500;
				text-decoration: none;
				border: 1px solid;
				cursor: pointer;
			}
			.gf-btn--ghost {
				color: var(--text-2, #adb5be);
				border-color: var(--border-subtle, #23282f);
				background: transparent;
			}
		</style>

		<div class="gf-page">
			<header class="gf-header">
				<a href={resolve('/steering?tab=fleet')} class="gf-back" aria-label="Back">←</a>
				<h2 class="gf-title">New steerer</h2>
			</header>

			<div class="gf-body">
				<nav class="gf-steps" aria-label="Create flow steps">
					<div class="gf-step gf-step--done"><span class="gf-step__num">✓</span> Pick session</div>
					<span class="gf-step-sep">›</span>
					<div class="gf-step gf-step--done">
						<span class="gf-step__num">✓</span> Session context
					</div>
					<span class="gf-step-sep">›</span>
					<div class="gf-step gf-step--done"><span class="gf-step__num">✓</span> Define steer</div>
					<span class="gf-step-sep">›</span>
					<div class="gf-step gf-step--active"><span class="gf-step__num">4</span> Confirm</div>
				</nav>

				<div>
					<h3 class="gf-step-heading">Create steerer and enrol session 64b50581</h3>
					<p class="gf-step-sub">
						Review the configuration below. When you click Create, the steerer is registered and
						immediately enrolled in session 64b50581. It will start watching the session's next
						turn.
					</p>
				</div>

				<div class="gf-enrol-target" role="note">
					<span class="gf-enrol-icon" aria-hidden="true">🔗</span>
					<p class="gf-enrol-text">
						This steerer will be enrolled in session
						<span class="gf-enrol-session-id">64b50581</span>
						immediately after creation.
					</p>
				</div>

				<div class="gf-summary">
					<div class="gf-summary-head">
						<span class="gf-summary-title">Configuration</span>
						<button type="button" class="gf-edit-link">Edit any field →</button>
					</div>
					<div class="gf-summary-rows">
						<div class="gf-summary-row">
							<span class="gf-summary-key">Display name</span>
							<span class="gf-summary-val">Phase 13 Steerer</span>
						</div>
						<div class="gf-summary-row">
							<span class="gf-summary-key">Slug</span>
							<span class="gf-summary-val">phase-13-steerer</span>
						</div>
						<div class="gf-summary-row">
							<span class="gf-summary-key">Thread anchor</span>
							<span class="gf-summary-val gf-summary-val--prompt">
								Implement the Phase 13 steering backend. Stay within crates/mike-features/src/steerer/. Do
								not touch frontend or migration files.
							</span>
						</div>
						<div class="gf-summary-row">
							<span class="gf-summary-key">Authority</span>
							<span class="gf-summary-val">supply context</span>
						</div>
						<div class="gf-summary-row">
							<span class="gf-summary-key">Mode</span>
							<span class="gf-summary-val">augment</span>
						</div>
						<div class="gf-summary-row">
							<span class="gf-summary-key">Model</span>
							<span class="gf-summary-val">openai/gpt-5.6-luna</span>
						</div>
						<div class="gf-summary-row">
							<span class="gf-summary-key">Confidence</span>
							<span class="gf-summary-val">0.7</span>
						</div>
						<div class="gf-summary-row">
							<span class="gf-summary-key">Debounce</span>
							<span class="gf-summary-val">2 turns</span>
						</div>
						<div class="gf-summary-row">
							<span class="gf-summary-key">Starts enabled</span>
							<span class="gf-summary-val">yes</span>
						</div>
					</div>
				</div>

				<button type="button" class="gf-btn-create"> Create + enrol session 64b50581 </button>

				<div class="gf-whats-next" role="note">
					<p class="gf-whats-next__title">After you click Create:</p>
					<ol class="gf-whats-next__steps">
						<li>The steerer is registered in the fleet.</li>
						<li>Session 64b50581 is enrolled in this steerer.</li>
						<li>The steerer starts evaluating the session's next turn.</li>
						<li>Events appear in the Monitor tab as they fire.</li>
					</ol>
					<p class="gf-whats-next__note">
						You can disenrol the session or disable the steerer at any time from the Fleet or
						Session page.
					</p>
				</div>

				<div class="gf-footer">
					<button type="button" class="gf-btn gf-btn--ghost">← Back to edit</button>
				</div>
			</div>
		</div>
	</div>
</Story>

<Story
	name="HelpDrawerWithSteering"
	parameters={{
		docs: {
			description: {
				story:
					'HelpDrawer open, showing the new "Steering" group (spec §13) with 4 links: How steering works, Session Steering section, Fleet, Monitor. Appears after How-to guides.'
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
				display: flex;
				justify-content: flex-end;
			}
			.hd-shell {
				width: 440px;
				height: 100vh;
				background: var(--surface, #161b22);
				border-left: 1px solid var(--border-color, #30363d);
				display: flex;
				flex-direction: column;
				overflow: hidden;
			}
			.hd-head {
				padding: 20px 24px 16px;
				border-bottom: 1px solid var(--border-subtle, #23282f);
				flex-shrink: 0;
			}
			.hd-title {
				margin: 0 0 4px;
				font-size: 1rem;
				font-weight: 600;
				color: var(--text-strong, #dde2e8);
			}
			.hd-sub {
				margin: 0;
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				line-height: 1.4;
			}
			.hd-body {
				flex: 1;
				overflow-y: auto;
				padding: 0;
			}
			.group {
				padding: 14px 20px 12px;
				border-bottom: 1px solid var(--border-subtle, #23282f);
			}
			.group-head {
				display: flex;
				align-items: center;
				gap: 6px;
				margin-bottom: 10px;
			}
			.group-icon {
				font-size: 14px;
				color: var(--text-3, #c0c7d0);
			}
			.group-label {
				margin: 0;
				font-size: 10px;
				font-weight: 600;
				text-transform: uppercase;
				letter-spacing: 0.08em;
				color: var(--text-3, #c0c7d0);
			}
			.group--steering {
				border-left: 2px solid var(--status-steering-border);
				padding-left: 18px;
			}
			.group--steering .group-label {
				color: var(--status-steering-fg);
			}
			.link-list {
				list-style: none;
				margin: 0;
				padding: 0;
				display: flex;
				flex-direction: column;
				gap: 2px;
			}
			.link-row {
				display: flex;
				flex-direction: column;
				gap: 2px;
				padding: 7px 10px;
				border-radius: 4px;
				text-decoration: none;
				color: inherit;
			}
			.link-row:hover {
				background: color-mix(in srgb, var(--surface-2, #1c232d) 70%, transparent 30%);
			}
			.link-title {
				display: flex;
				align-items: center;
				gap: 4px;
				font-size: 12px;
				color: var(--text-1, #c2c9d1);
				font-weight: 500;
			}
			.link-desc {
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				line-height: 1.4;
			}
			.badge-new {
				display: inline-flex;
				align-items: center;
				padding: 1px 5px;
				background: var(--status-steering-bg);
				border: 1px solid var(--status-steering-border);
				border-radius: 3px;
				font-size: 9px;
				font-weight: 700;
				color: var(--status-steering-fg);
				text-transform: uppercase;
				letter-spacing: 0.05em;
				margin-left: 6px;
			}
		</style>

		<div class="hd-shell" role="dialog" aria-label="Help & references">
			<div class="hd-head">
				<h2 class="hd-title">Help & references</h2>
				<p class="hd-sub">Project documentation, upstream context, and proxy diagnostics.</p>
			</div>

			<div class="hd-body">
				<section class="group" aria-labelledby="hd-guides">
					<header class="group-head">
						<span class="group-icon" aria-hidden="true">🎓</span>
						<h3 id="hd-guides" class="group-label">How-to guides</h3>
					</header>
					<ul class="link-list">
						<li>
							<a class="link-row" href={resolve('/help/guides/getting-started')}>
								<span class="link-title">Get Claude Code talking to the proxy</span>
								<span class="link-desc"
									>Point ANTHROPIC_BASE_URL at the proxy, confirm requests land in the SPA.</span
								>
							</a>
						</li>
						<li>
							<a class="link-row" href={resolve('/help/guides/inspecting-requests')}>
								<span class="link-title">Inspect a captured request</span>
								<span class="link-desc"
									>Walk through the request detail view: messages, system prompt, response,
									findings.</span
								>
							</a>
						</li>
						<li>
							<a class="link-row" href={resolve('/help/guides/filter-rules')}>
								<span class="link-title">Rewrite outbound system prompts</span>
								<span class="link-desc"
									>Add, edit, and order filter rules — drop, replace, or append spans of the system
									block.</span
								>
							</a>
						</li>
						<li>
							<a class="link-row" href={resolve('/help/guides/steering-concepts')}>
								<span class="link-title">
									How steering works
									<span class="badge-new" aria-label="new guide">new</span>
								</span>
								<span class="link-desc"
									>What steerers are, the lifecycle, authority and mode explained with examples.</span
								>
							</a>
						</li>
					</ul>
				</section>

				<section class="group group--steering" aria-labelledby="hd-steering">
					<header class="group-head">
						<span class="group-icon" aria-hidden="true">🤖</span>
						<h3 id="hd-steering" class="group-label">Steering</h3>
					</header>
					<ul class="link-list">
						<li>
							<a class="link-row" href={resolve('/help/guides/steering-concepts')}>
								<span class="link-title">How steering works</span>
								<span class="link-desc"
									>What steerers are, the lifecycle, authority, and mode explained.</span
								>
							</a>
						</li>
						<li>
							<a class="link-row" href={resolve('/sessions')}>
								<span class="link-title">Session Steering section</span>
								<span class="link-desc"
									>Open a session to see its steerer enrolments and firing history.</span
								>
							</a>
						</li>
						<li>
							<a class="link-row" href={resolve('/steering?tab=fleet')}>
								<span class="link-title">Fleet</span>
								<span class="link-desc">Register, edit, and enable steerers.</span>
							</a>
						</li>
						<li>
							<a class="link-row" href={resolve('/steering?tab=monitor')}>
								<span class="link-title">Monitor</span>
								<span class="link-desc">Live cross-session intervention feed.</span>
							</a>
						</li>
					</ul>
				</section>

				<section class="group" aria-labelledby="hd-docs">
					<header class="group-head">
						<span class="group-icon" aria-hidden="true">📖</span>
						<h3 id="hd-docs" class="group-label">Documentation</h3>
					</header>
					<ul class="link-list">
						<li>
							<a class="link-row" href={resolve('/help')}>
								<span class="link-title">Help index</span>
								<span class="link-desc"
									>Landing page — how-to guides, glossary, and external docs.</span
								>
							</a>
						</li>
						<li>
							<a class="link-row" href={resolve('/help/glossary')}>
								<span class="link-title">Glossary</span>
								<span class="link-desc">Single source of truth for vocabulary in mike.</span>
							</a>
						</li>
					</ul>
				</section>
			</div>
		</div>
	</div>
</Story>

<Story
	name="FleetEnhancedEmpty"
	parameters={{
		docs: {
			description: {
				story:
					'Fleet empty state (enhanced): Bot icon, steerer definition, Create CTA, plus secondary "How does steering work?" link to the guide. Per spec §14.2.'
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
			.st-page {
				display: flex;
				flex-direction: column;
				height: 100vh;
			}
			.st-header {
				display: flex;
				align-items: flex-start;
				justify-content: space-between;
				gap: 12px;
				padding: 20px 24px 16px;
				border-bottom: 1px solid var(--border-color, #30363d);
			}
			.st-header__left {
				display: flex;
				flex-direction: column;
				gap: 2px;
			}
			.st-title {
				margin: 0;
				font-size: 1.1rem;
				font-weight: 600;
				color: var(--text-strong, #dde2e8);
			}
			.st-subtitle {
				margin: 0;
				font-size: 12px;
				color: var(--text-3, #c0c7d0);
			}
			.st-btn--primary {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 6px 12px;
				border-radius: 4px;
				font-size: 12px;
				font-family: var(--font-mono, monospace);
				font-weight: 500;
				text-decoration: none;
				border: 1px solid var(--status-steering-border);
				color: var(--status-steering-fg);
				background: var(--status-steering-bg);
			}
			.se-empty-state {
				display: flex;
				flex-direction: column;
				align-items: center;
				justify-content: center;
				gap: 12px;
				padding: 80px 32px;
				text-align: center;
				flex: 1;
			}
			.se-empty-state__icon {
				color: var(--text-3, #c0c7d0);
				font-size: 40px;
				line-height: 1;
				margin-bottom: 4px;
			}
			.se-empty-state__heading {
				margin: 0;
				font-family: var(--font-mono, monospace);
				font-size: 14px;
				font-weight: 600;
				color: var(--text-strong, #dde2e8);
			}
			.se-empty-state__body {
				margin: 0;
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				color: var(--text-2, #adb5be);
				max-width: 360px;
				line-height: 1.6;
			}
			.se-empty-state__action {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 7px 16px;
				border-radius: 4px;
				font-size: 12px;
				font-family: var(--font-mono, monospace);
				font-weight: 500;
				text-decoration: none;
				border: 1px solid var(--status-steering-border);
				color: var(--status-steering-fg);
				background: var(--status-steering-bg);
				margin-top: 4px;
			}
			.se-empty-state__guide-link {
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				text-decoration: none;
				display: flex;
				align-items: center;
				gap: 4px;
			}
			.se-empty-state__guide-link:hover {
				color: var(--text-1, #c2c9d1);
				text-decoration: underline;
			}
		</style>

		<section class="st-page">
			<header class="st-header">
				<div class="st-header__left">
					<h2 class="st-title">Steerer Fleet</h2>
					<p class="st-subtitle">
						Registered steerers — manage the team, edit prompts, enrol sessions.
					</p>
				</div>
				<div>
					<a href={resolve('steering/new')} class="st-btn--primary">+ New steerer</a>
				</div>
			</header>

			<div class="se-empty-state">
				<div class="se-empty-state__icon" aria-hidden="true">🤖</div>
				<p class="se-empty-state__heading">No steerers registered yet</p>
				<p class="se-empty-state__body">
					A steerer is a lightweight LLM agent that monitors a live session for drift and can inject
					corrective messages when the session wanders off track.
				</p>
				<a href={resolve('steering/new')} class="se-empty-state__action">+ New steerer</a>
				<a href={resolve('/help/guides/steering-concepts')} class="se-empty-state__guide-link">
					How does steering work? ↗
				</a>
			</div>
		</section>
	</div>
</Story>

<Story
	name="MonitorEnhancedEmpty"
	parameters={{
		docs: {
			description: {
				story:
					'Monitor empty state (enhanced): Radio icon, enrolment instructions, Fleet CTA, plus secondary "What does an event look like?" lifecycle link. Per spec §14.3.'
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
			.sm-panel {
				display: flex;
				flex-direction: column;
				gap: 12px;
				padding: 20px;
				height: 100vh;
				box-sizing: border-box;
			}
			.sm-header {
				display: flex;
				align-items: center;
				gap: 8px;
			}
			.sm-dot {
				width: 8px;
				height: 8px;
				border-radius: 50%;
				background: var(--status-steering-fg);
				box-shadow: 0 0 6px var(--status-steering-fg);
			}
			.sm-title {
				font-size: 11px;
				font-weight: 600;
				text-transform: uppercase;
				letter-spacing: 0.06em;
				color: var(--text-3, #c0c7d0);
			}
			.sm-enrolled-badge {
				display: inline-flex;
				align-items: center;
				padding: 1px 6px;
				border: 1px solid var(--status-steering-border);
				border-radius: 4px;
				background: var(--status-steering-bg);
				font-size: 10px;
				font-weight: 600;
				color: var(--status-steering-fg);
				text-transform: uppercase;
			}
			.se-empty-state {
				display: flex;
				flex-direction: column;
				align-items: center;
				justify-content: center;
				gap: 12px;
				padding: 60px 32px;
				text-align: center;
				flex: 1;
			}
			.se-empty-state__icon {
				color: var(--text-3, #c0c7d0);
				font-size: 36px;
				line-height: 1;
				margin-bottom: 4px;
			}
			.se-empty-state__heading {
				margin: 0;
				font-family: var(--font-mono, monospace);
				font-size: 14px;
				font-weight: 600;
				color: var(--text-strong, #dde2e8);
			}
			.se-empty-state__body {
				margin: 0;
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				color: var(--text-2, #adb5be);
				max-width: 360px;
				line-height: 1.6;
			}
			.se-empty-state__action {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 6px 14px;
				border-radius: 4px;
				font-size: 12px;
				font-family: var(--font-mono, monospace);
				font-weight: 500;
				text-decoration: none;
				border: 1px solid var(--border-subtle, #23282f);
				color: var(--text-1, #c2c9d1);
				background: var(--surface-2, #1c232d);
				margin-top: 4px;
			}
			.se-empty-state__action:hover {
				border-color: var(--status-steering-border);
				color: var(--status-steering-fg);
			}
			.se-empty-state__guide-link {
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				text-decoration: none;
				display: flex;
				align-items: center;
				gap: 4px;
			}
			.se-empty-state__guide-link:hover {
				color: var(--text-1, #c2c9d1);
				text-decoration: underline;
			}
		</style>

		<div class="sm-panel">
			<div class="sm-header">
				<span class="sm-dot" aria-hidden="true"></span>
				<span class="sm-title">LIVE STEERING MONITOR</span>
				<span class="sm-enrolled-badge">0 sessions enrolled</span>
			</div>

			<div class="se-empty-state">
				<div class="se-empty-state__icon" aria-hidden="true">📡</div>
				<p class="se-empty-state__heading">No sessions enrolled for steering</p>
				<p class="se-empty-state__body">
					To see live events here, go to a live session and enrol it, or create a steerer in the
					Fleet tab to get started.
				</p>
				<a href={resolve('/steering?tab=fleet')} class="se-empty-state__action">→ Go to Fleet</a>
				<a
					href={resolve('/help/guides/steering-concepts#the-lifecycle')}
					class="se-empty-state__guide-link"
				>
					What does an event look like? ↗
				</a>
			</div>
		</div>
	</div>
</Story>

<Story
	name="MonitorEmptyNoEnrolments"
	parameters={{
		docs: {
			description: {
				story:
					'Monitor tab empty state when no sessions are enrolled for steering. Radio icon, explanation, action to go to Fleet.'
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
			.sm-panel {
				display: flex;
				flex-direction: column;
				gap: 12px;
				padding: 20px;
				height: 100vh;
				box-sizing: border-box;
			}
			.sm-header {
				display: flex;
				align-items: center;
				gap: 8px;
			}
			.sm-dot {
				width: 8px;
				height: 8px;
				border-radius: 50%;
				background: var(--status-steering-fg);
				box-shadow: 0 0 6px var(--status-steering-fg);
			}
			.sm-title {
				font-family: var(--font-mono, monospace);
				font-size: 11px;
				font-weight: 600;
				text-transform: uppercase;
				letter-spacing: 0.06em;
				color: var(--text-3, #c0c7d0);
			}
			.sm-enrolled-badge {
				display: inline-flex;
				align-items: center;
				padding: 1px 6px;
				border: 1px solid var(--status-steering-border);
				border-radius: 4px;
				background: var(--status-steering-bg);
				font-family: var(--font-mono, monospace);
				font-size: 10px;
				font-weight: 600;
				color: var(--status-steering-fg);
				text-transform: uppercase;
			}
			.se-empty-state {
				display: flex;
				flex-direction: column;
				align-items: center;
				justify-content: center;
				gap: 12px;
				padding: 60px 32px;
				text-align: center;
				flex: 1;
			}
			.se-empty-state__icon {
				color: var(--text-3, #c0c7d0);
				font-size: 36px;
				line-height: 1;
				margin-bottom: 4px;
			}
			.se-empty-state__heading {
				margin: 0;
				font-family: var(--font-mono, monospace);
				font-size: 14px;
				font-weight: 600;
				color: var(--text-strong, #dde2e8);
			}
			.se-empty-state__body {
				margin: 0;
				font-family: var(--font-mono, monospace);
				font-size: 12px;
				color: var(--text-2, #adb5be);
				max-width: 340px;
				line-height: 1.6;
			}
			.se-empty-state__action {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 6px 14px;
				border-radius: 4px;
				font-size: 12px;
				font-family: var(--font-mono, monospace);
				font-weight: 500;
				text-decoration: none;
				border: 1px solid var(--border-subtle, #23282f);
				color: var(--text-1, #c2c9d1);
				background: var(--surface-2, #1c232d);
				margin-top: 4px;
			}
			.se-empty-state__action:hover {
				border-color: var(--status-steering-border);
				color: var(--status-steering-fg);
			}
		</style>

		<div class="sm-panel">
			<div class="sm-header">
				<span class="sm-dot" aria-hidden="true"></span>
				<span class="sm-title">LIVE STEERING MONITOR</span>
				<span class="sm-enrolled-badge">0 sessions enrolled</span>
			</div>

			<div class="se-empty-state">
				<div class="se-empty-state__icon" aria-hidden="true">📡</div>
				<p class="se-empty-state__heading">No sessions enrolled for steering</p>
				<p class="se-empty-state__body">
					To see live events here, open a session and enrol it from the Session Steering section, or
					enable a steerer in the Fleet tab.
				</p>
				<a href={resolve('/steering?tab=fleet')} class="se-empty-state__action"> → Go to Fleet </a>
			</div>
		</div>
	</div>
</Story>
