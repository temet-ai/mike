<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { resolve } from '$app/paths';
	import EmptyStateOnboarding from '$lib/components/shared/EmptyStateOnboarding.svelte';

	const { Story } = defineMeta({
		title: 'Draft/SteererEditor',
		component: EmptyStateOnboarding,
		tags: ['autodocs'],
		parameters: {
			layout: 'fullscreen',
			docs: {
				description: {
					component:
						'Visual spec for the /steering/[id] editor page. ' +
						'Static HTML — not the live page. ' +
						'See docs/plans/scalable-noodling-squirrel.md §Phase D.'
				}
			}
		}
	});
</script>

<Story
	name="LoadedSafe"
	parameters={{
		docs: {
			description: {
				story:
					'Steerer loaded with supply_context authority and augment mode — safe state, no warning box.'
			}
		}
	}}
>
	<div class="sb-wrapper">
		<style>
			.sb-wrapper {
				background: var(--bg-base, #0f1117);
				min-height: 100vh;
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-sans, system-ui, sans-serif);
				font-size: 13px;
			}
			.se-page {
				max-width: 760px;
				margin: 0 auto;
				padding: 24px 20px 48px;
			}
			.se-header {
				display: flex;
				align-items: flex-start;
				gap: 10px;
				margin-bottom: 24px;
				border-bottom: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.08));
				padding-bottom: 16px;
			}
			.se-back {
				display: flex;
				align-items: center;
				justify-content: center;
				width: 28px;
				height: 28px;
				border-radius: 6px;
				border: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.08));
				color: var(--text-2, #adb5be);
				text-decoration: none;
				flex-shrink: 0;
				margin-top: 2px;
			}
			.se-back:hover {
				border-color: var(--border-muted, rgba(255, 255, 255, 0.16));
				color: var(--text-1, #c2c9d1);
			}
			.se-header__titles {
				flex: 1;
			}
			.se-title {
				margin: 0 0 2px;
				font-size: 16px;
				font-weight: 600;
				color: var(--text-strong, #dde2e8);
			}
			.se-slug {
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				font-family: var(--font-mono, monospace);
			}
			.se-header__actions {
				display: flex;
				gap: 6px;
				align-items: center;
				margin-left: auto;
			}
			.se-btn {
				display: inline-flex;
				align-items: center;
				gap: 4px;
				padding: 5px 10px;
				border-radius: 6px;
				font-size: 12px;
				cursor: pointer;
				border: 1px solid var(--border-muted, rgba(255, 255, 255, 0.16));
				background: transparent;
				color: var(--text-2, #adb5be);
				text-decoration: none;
			}
			.se-btn:hover {
				border-color: var(--rose, #b8849e);
				color: var(--rose, #b8849e);
			}
			.se-btn--danger {
				border-color: var(--red-dim, rgba(199, 73, 73, 0.35));
				color: var(--red, #c74949);
			}
			.se-btn--danger:hover {
				background: var(--red-dim, rgba(199, 73, 73, 0.15));
			}
			.se-body {
				display: flex;
				flex-direction: column;
				gap: 16px;
			}
			.se-fieldset {
				border: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.08));
				border-radius: 8px;
				padding: 16px;
				margin: 0;
				display: flex;
				flex-direction: column;
				gap: 14px;
			}
			.se-legend {
				font-size: 11px;
				font-weight: 600;
				letter-spacing: 0.06em;
				text-transform: uppercase;
				color: var(--text-2, #adb5be);
				padding: 0 4px;
			}
			.se-legend__note {
				font-weight: 400;
				text-transform: none;
				letter-spacing: 0;
				color: var(--text-3, #c0c7d0);
			}
			.se-field {
				display: flex;
				flex-direction: column;
				gap: 4px;
			}
			.se-field--row {
				flex-direction: row;
				align-items: center;
				justify-content: space-between;
			}
			.se-label {
				font-size: 12px;
				color: var(--text-2, #adb5be);
				font-weight: 500;
			}
			.se-input,
			.se-select,
			.se-textarea {
				background: var(--bg-2, #1a1d26);
				border: 1px solid var(--border-muted, rgba(255, 255, 255, 0.12));
				border-radius: 6px;
				color: var(--text-1, #c2c9d1);
				font-size: 13px;
				padding: 7px 10px;
				outline: none;
			}
			.se-input:focus,
			.se-select:focus,
			.se-textarea:focus {
				border-color: var(--rose, #b8849e);
			}
			.se-textarea {
				resize: vertical;
				min-height: 72px;
				font-family: inherit;
			}
			.se-select {
				cursor: pointer;
			}
			.se-select--danger {
				border-color: var(--amber, #c49060) !important;
				color: var(--amber, #c49060);
			}
			.se-toggle {
				display: inline-flex;
				align-items: center;
				gap: 8px;
				background: transparent;
				border: none;
				cursor: pointer;
				padding: 0;
			}
			.se-toggle__track {
				width: 36px;
				height: 20px;
				border-radius: 10px;
				border: 1px solid var(--border-muted, rgba(255, 255, 255, 0.16));
				background: var(--bg-3, #20232e);
				position: relative;
				flex-shrink: 0;
			}
			.se-toggle--on .se-toggle__track {
				background: var(--rose-dim, rgba(184, 132, 158, 0.25));
				border-color: var(--rose, #b8849e);
			}
			.se-toggle__thumb {
				position: absolute;
				top: 2px;
				left: 2px;
				width: 14px;
				height: 14px;
				border-radius: 50%;
				background: var(--text-3, #c0c7d0);
				transition: transform 0.15s;
			}
			.se-toggle--on .se-toggle__thumb {
				transform: translateX(16px);
				background: var(--rose, #b8849e);
			}
			.se-toggle__label {
				font-size: 12px;
				color: var(--text-2, #adb5be);
			}
			.se-field--grid-2 {
				display: grid;
				grid-template-columns: 1fr 1fr;
				gap: 12px;
			}
			.se-footer {
				display: flex;
				align-items: center;
				gap: 10px;
				margin-top: 8px;
				padding-top: 16px;
				border-top: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.08));
			}
			.se-save-btn {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 7px 16px;
				border-radius: 6px;
				font-size: 13px;
				font-weight: 500;
				cursor: pointer;
				border: none;
				background: var(--rose, #b8849e);
				color: #fff;
			}
			.se-save-btn:disabled {
				opacity: 0.5;
				cursor: not-allowed;
			}
		</style>
		<div class="se-page">
			<header class="se-header">
				<a href={resolve('/steering?tab=fleet')} class="se-back" aria-label="Back to steerer fleet"
					>←</a
				>
				<div class="se-header__titles">
					<h2 class="se-title">Refocus</h2>
					<span class="se-slug">refocus</span>
				</div>
				<div class="se-header__actions">
					<a href={resolve('agents/3')} class="se-btn" target="_blank" rel="noopener"
						>↗ Edit prompt</a
					>
					<button type="button" class="se-btn se-btn--danger">🗑 Delete</button>
				</div>
			</header>

			<div class="se-body">
				<fieldset class="se-fieldset">
					<legend class="se-legend">Basic information</legend>
					<div class="se-field">
						<label class="se-label" for="s-dn">Display name</label>
						<input id="s-dn" type="text" class="se-input" value="Refocus" />
					</div>
					<div class="se-field">
						<label class="se-label" for="s-scope">Project scope</label>
						<select id="s-scope" class="se-select">
							<option value="" selected>Global (all projects)</option>
							<option value="/Users/dev/projects/my-project">/Users/dev/projects/my-project</option>
							<option value="/Users/dev/projects/other-project"
								>/Users/dev/projects/other-project</option
							>
						</select>
					</div>
					<div class="se-field">
						<label class="se-label" for="s-desc">Description</label>
						<textarea id="s-desc" class="se-textarea" rows="3"
							>Re-focuses the session when it drifts away from the primary goal. Monitors for scope creep and injects a corrective reminder.</textarea
						>
					</div>
					<div class="se-field se-field--row">
						<label class="se-label" for="s-en">Enabled</label>
						<button id="s-en" type="button" class="se-toggle se-toggle--on" aria-pressed="true">
							<span class="se-toggle__track" aria-hidden="true"
								><span class="se-toggle__thumb"></span></span
							>
							<span class="se-toggle__label">Enabled</span>
						</button>
					</div>
				</fieldset>

				<fieldset class="se-fieldset">
					<legend class="se-legend">Abilities</legend>
					<div class="se-field">
						<label class="se-label" for="s-auth">Authority</label>
						<select id="s-auth" class="se-select">
							<option value="supply_context" selected>Supply context</option>
							<option value="redirect">Redirect</option>
							<option value="command">⚠ Command (high-impact)</option>
						</select>
					</div>
					<div class="se-field">
						<label class="se-label" for="s-mode">Mode</label>
						<select id="s-mode" class="se-select">
							<option value="augment" selected>Augment</option>
							<option value="drive">Drive (high-impact)</option>
						</select>
					</div>
				</fieldset>

				<fieldset class="se-fieldset">
					<legend class="se-legend">Configuration</legend>
					<div class="se-field se-field--grid-2">
						<div class="se-field">
							<label class="se-label" for="s-ct">Confidence threshold</label>
							<input
								id="s-ct"
								type="number"
								class="se-input"
								min="0"
								max="1"
								step="0.01"
								value="0.70"
							/>
						</div>
						<div class="se-field">
							<label class="se-label" for="s-dt">Debounce turns</label>
							<input id="s-dt" type="number" class="se-input" min="0" step="1" value="2" />
						</div>
					</div>
					<div class="se-field se-field--grid-2">
						<div class="se-field">
							<label class="se-label" for="s-model">Model</label>
							<input id="s-model" type="text" class="se-input" value="claude-haiku-4-5" />
						</div>
						<div class="se-field">
							<label class="se-label" for="s-pri">Priority</label>
							<input id="s-pri" type="number" class="se-input" step="1" value="100" />
						</div>
					</div>
				</fieldset>

				<div class="se-footer">
					<button type="button" class="se-save-btn">💾 Save changes</button>
				</div>
			</div>
		</div>
	</div>
</Story>

<Story
	name="DangerWarning"
	parameters={{
		docs: {
			description: {
				story: 'Steerer with command authority + drive mode — prominent amber warning box rendered.'
			}
		}
	}}
>
	<div class="sb-wrapper">
		<style>
			.sb-wrapper {
				background: var(--bg-base, #0f1117);
				min-height: 100vh;
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-sans, system-ui, sans-serif);
				font-size: 13px;
			}
			.se-page {
				max-width: 760px;
				margin: 0 auto;
				padding: 24px 20px 48px;
			}
			.se-header {
				display: flex;
				align-items: flex-start;
				gap: 10px;
				margin-bottom: 24px;
				border-bottom: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.08));
				padding-bottom: 16px;
			}
			.se-back {
				display: flex;
				align-items: center;
				justify-content: center;
				width: 28px;
				height: 28px;
				border-radius: 6px;
				border: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.08));
				color: var(--text-2, #adb5be);
				text-decoration: none;
				flex-shrink: 0;
				margin-top: 2px;
			}
			.se-header__titles {
				flex: 1;
			}
			.se-title {
				margin: 0 0 2px;
				font-size: 16px;
				font-weight: 600;
				color: var(--text-strong, #dde2e8);
			}
			.se-slug {
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				font-family: var(--font-mono, monospace);
			}
			.se-header__actions {
				display: flex;
				gap: 6px;
				align-items: center;
				margin-left: auto;
			}
			.se-btn {
				display: inline-flex;
				align-items: center;
				gap: 4px;
				padding: 5px 10px;
				border-radius: 6px;
				font-size: 12px;
				cursor: pointer;
				border: 1px solid var(--border-muted, rgba(255, 255, 255, 0.16));
				background: transparent;
				color: var(--text-2, #adb5be);
				text-decoration: none;
			}
			.se-btn--danger {
				border-color: var(--red-dim, rgba(199, 73, 73, 0.35));
				color: var(--red, #c74949);
			}
			.se-body {
				display: flex;
				flex-direction: column;
				gap: 16px;
			}
			.se-warning-box {
				border: 1px solid var(--amber, #c49060);
				border-radius: 8px;
				background: rgba(196, 144, 96, 0.08);
				padding: 14px 16px;
			}
			.se-warning-box__head {
				display: flex;
				align-items: center;
				gap: 8px;
				margin-bottom: 10px;
				color: var(--amber, #c49060);
				font-weight: 600;
				font-size: 13px;
			}
			.se-warning-box__list {
				margin: 0 0 10px;
				padding: 0 0 0 4px;
				list-style: none;
				display: flex;
				flex-direction: column;
				gap: 8px;
			}
			.se-warning-box__list li {
				display: flex;
				gap: 8px;
				color: var(--text-1, #c2c9d1);
				font-size: 12px;
				line-height: 1.5;
			}
			.se-warning-box__list li svg,
			.se-warning-box__list li .icon {
				color: var(--amber, #c49060);
				flex-shrink: 0;
				margin-top: 2px;
			}
			.se-warning-box__footer {
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				margin: 0;
				border-top: 1px solid rgba(196, 144, 96, 0.2);
				padding-top: 8px;
			}
			.se-fieldset {
				border: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.08));
				border-radius: 8px;
				padding: 16px;
				margin: 0;
				display: flex;
				flex-direction: column;
				gap: 14px;
			}
			.se-legend {
				font-size: 11px;
				font-weight: 600;
				letter-spacing: 0.06em;
				text-transform: uppercase;
				color: var(--text-2, #adb5be);
				padding: 0 4px;
			}
			.se-legend__note {
				font-weight: 400;
				text-transform: none;
				letter-spacing: 0;
				color: var(--text-3, #c0c7d0);
			}
			.se-field {
				display: flex;
				flex-direction: column;
				gap: 4px;
			}
			.se-label {
				font-size: 12px;
				color: var(--text-2, #adb5be);
				font-weight: 500;
			}
			.se-select {
				background: var(--bg-2, #1a1d26);
				border: 1px solid var(--border-muted, rgba(255, 255, 255, 0.12));
				border-radius: 6px;
				color: var(--text-1, #c2c9d1);
				font-size: 13px;
				padding: 7px 10px;
				cursor: pointer;
			}
			.se-select--danger {
				border-color: var(--amber, #c49060) !important;
				color: var(--amber, #c49060);
			}
			.se-footer {
				display: flex;
				align-items: center;
				gap: 10px;
				margin-top: 8px;
				padding-top: 16px;
				border-top: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.08));
			}
			.se-save-btn {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 7px 16px;
				border-radius: 6px;
				font-size: 13px;
				font-weight: 500;
				cursor: pointer;
				border: none;
				background: var(--rose, #b8849e);
				color: #fff;
			}
		</style>
		<div class="se-page">
			<header class="se-header">
				<a href={resolve('/steering?tab=fleet')} class="se-back" aria-label="Back to steerer fleet"
					>←</a
				>
				<div class="se-header__titles">
					<h2 class="se-title">Override Agent</h2>
					<span class="se-slug">override-agent</span>
				</div>
				<div class="se-header__actions">
					<a href={resolve('agents/7')} class="se-btn" target="_blank" rel="noopener"
						>↗ Edit prompt</a
					>
					<button type="button" class="se-btn se-btn--danger">🗑 Delete</button>
				</div>
			</header>

			<div class="se-body">
				<div class="se-warning-box" role="alert">
					<div class="se-warning-box__head">
						🛡 <strong>High-impact mode active</strong>
					</div>
					<ul class="se-warning-box__list">
						<li>
							<span class="icon">⚡</span>
							<span
								><strong>Command authority</strong> — the steerer can issue binding directives to the
								target session. The session's Claude Code process will act on these messages as if they
								came from the operator. Reserve for trusted, well-tested steerers only.</span
							>
						</li>
						<li>
							<span class="icon">⚡</span>
							<span
								><strong>Drive mode</strong> — the steerer actively redirects the session's next turn,
								overriding the user's intended direction. This is the most invasive steering mode.</span
							>
						</li>
					</ul>
					<p class="se-warning-box__footer">
						Ensure the steerer's prompt is carefully reviewed before enabling.
					</p>
				</div>

				<fieldset class="se-fieldset">
					<legend class="se-legend">Abilities</legend>
					<div class="se-field">
						<label class="se-label" for="d-auth">Authority</label>
						<select id="d-auth" class="se-select se-select--danger">
							<option value="supply_context">Supply context</option>
							<option value="redirect">Redirect</option>
							<option value="command" selected>⚠ Command (high-impact)</option>
						</select>
						<span style="font-size:11px;color:var(--amber,#c49060);margin-top:2px;"
							>The steerer can send binding directives to the session.</span
						>
					</div>
					<div class="se-field">
						<label class="se-label" for="d-mode">Mode</label>
						<select id="d-mode" class="se-select se-select--danger">
							<option value="augment">Augment</option>
							<option value="drive" selected>Drive (high-impact)</option>
						</select>
						<span style="font-size:11px;color:var(--rose,#b8849e);margin-top:2px;"
							>Steerer actively redirects the session's next turn.</span
						>
					</div>
				</fieldset>

				<div class="se-footer">
					<button type="button" class="se-save-btn">💾 Save changes</button>
				</div>
			</div>
		</div>
	</div>
</Story>

<Story
	name="SavedOk"
	parameters={{
		docs: {
			description: {
				story: 'Steerer saved successfully — green flash banner displayed, no warnings.'
			}
		}
	}}
>
	<div class="sb-wrapper">
		<style>
			.sb-wrapper {
				background: var(--bg-base, #0f1117);
				min-height: 100vh;
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-sans, system-ui, sans-serif);
				font-size: 13px;
			}
			.se-page {
				max-width: 760px;
				margin: 0 auto;
				padding: 24px 20px 48px;
			}
			.se-header {
				display: flex;
				align-items: flex-start;
				gap: 10px;
				margin-bottom: 24px;
				border-bottom: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.08));
				padding-bottom: 16px;
			}
			.se-back {
				display: flex;
				align-items: center;
				justify-content: center;
				width: 28px;
				height: 28px;
				border-radius: 6px;
				border: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.08));
				color: var(--text-2, #adb5be);
				text-decoration: none;
				flex-shrink: 0;
				margin-top: 2px;
			}
			.se-header__titles {
				flex: 1;
			}
			.se-title {
				margin: 0 0 2px;
				font-size: 16px;
				font-weight: 600;
				color: var(--text-strong, #dde2e8);
			}
			.se-slug {
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				font-family: var(--font-mono, monospace);
			}
			.se-body {
				display: flex;
				flex-direction: column;
				gap: 16px;
			}
			.se-banner {
				border-radius: 7px;
				padding: 10px 14px;
				font-size: 13px;
			}
			.se-banner--ok {
				background: rgba(64, 160, 100, 0.12);
				border: 1px solid rgba(64, 160, 100, 0.3);
				color: var(--green, #40a064);
			}
			.se-fieldset {
				border: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.08));
				border-radius: 8px;
				padding: 16px;
				margin: 0;
				display: flex;
				flex-direction: column;
				gap: 14px;
			}
			.se-legend {
				font-size: 11px;
				font-weight: 600;
				letter-spacing: 0.06em;
				text-transform: uppercase;
				color: var(--text-2, #adb5be);
				padding: 0 4px;
			}
			.se-field {
				display: flex;
				flex-direction: column;
				gap: 4px;
			}
			.se-label {
				font-size: 12px;
				color: var(--text-2, #adb5be);
				font-weight: 500;
			}
			.se-input {
				background: var(--bg-2, #1a1d26);
				border: 1px solid var(--border-muted, rgba(255, 255, 255, 0.12));
				border-radius: 6px;
				color: var(--text-1, #c2c9d1);
				font-size: 13px;
				padding: 7px 10px;
			}
			.se-footer {
				display: flex;
				align-items: center;
				gap: 10px;
				margin-top: 8px;
				padding-top: 16px;
				border-top: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.08));
			}
			.se-save-btn {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 7px 16px;
				border-radius: 6px;
				font-size: 13px;
				font-weight: 500;
				cursor: pointer;
				border: none;
				background: var(--rose, #b8849e);
				color: #fff;
			}
		</style>
		<div class="se-page">
			<header class="se-header">
				<a href={resolve('/steering?tab=fleet')} class="se-back" aria-label="Back">←</a>
				<div class="se-header__titles">
					<h2 class="se-title">Refocus</h2>
					<span class="se-slug">refocus</span>
				</div>
			</header>
			<div class="se-body">
				<div class="se-banner se-banner--ok" role="status">Saved.</div>

				<fieldset class="se-fieldset">
					<legend class="se-legend">Basic information</legend>
					<div class="se-field">
						<label class="se-label" for="sok-dn">Display name</label>
						<input id="sok-dn" type="text" class="se-input" value="Refocus" />
					</div>
				</fieldset>

				<div class="se-footer">
					<button type="button" class="se-save-btn">💾 Save changes</button>
				</div>
			</div>
		</div>
	</div>
</Story>

<Story
	name="SavedWarnings"
	parameters={{
		docs: {
			description: {
				story:
					'Saved but envelope returned warnings — all warnings rendered in full, no truncation.'
			}
		}
	}}
>
	<div class="sb-wrapper">
		<style>
			.sb-wrapper {
				background: var(--bg-base, #0f1117);
				min-height: 100vh;
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-sans, system-ui, sans-serif);
				font-size: 13px;
			}
			.se-page {
				max-width: 760px;
				margin: 0 auto;
				padding: 24px 20px 48px;
			}
			.se-header {
				display: flex;
				align-items: flex-start;
				gap: 10px;
				margin-bottom: 24px;
				border-bottom: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.08));
				padding-bottom: 16px;
			}
			.se-back {
				display: flex;
				align-items: center;
				justify-content: center;
				width: 28px;
				height: 28px;
				border-radius: 6px;
				border: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.08));
				color: var(--text-2, #adb5be);
				text-decoration: none;
				flex-shrink: 0;
				margin-top: 2px;
			}
			.se-header__titles {
				flex: 1;
			}
			.se-title {
				margin: 0 0 2px;
				font-size: 16px;
				font-weight: 600;
				color: var(--text-strong, #dde2e8);
			}
			.se-slug {
				font-size: 11px;
				color: var(--text-3, #c0c7d0);
				font-family: var(--font-mono, monospace);
			}
			.se-body {
				display: flex;
				flex-direction: column;
				gap: 16px;
			}
			.se-banner {
				border-radius: 7px;
				padding: 10px 14px;
				font-size: 13px;
			}
			.se-banner--warn {
				background: rgba(196, 144, 96, 0.1);
				border: 1px solid rgba(196, 144, 96, 0.35);
				color: var(--amber, #c49060);
			}
			.se-warning-list {
				margin: 6px 0 0 16px;
				padding: 0;
				color: var(--text-1, #c2c9d1);
			}
			.se-warning-list li {
				margin-bottom: 4px;
				font-size: 12px;
			}
			.se-fieldset {
				border: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.08));
				border-radius: 8px;
				padding: 16px;
				margin: 0;
				display: flex;
				flex-direction: column;
				gap: 14px;
			}
			.se-legend {
				font-size: 11px;
				font-weight: 600;
				letter-spacing: 0.06em;
				text-transform: uppercase;
				color: var(--text-2, #adb5be);
				padding: 0 4px;
			}
			.se-field {
				display: flex;
				flex-direction: column;
				gap: 4px;
			}
			.se-label {
				font-size: 12px;
				color: var(--text-2, #adb5be);
				font-weight: 500;
			}
			.se-input {
				background: var(--bg-2, #1a1d26);
				border: 1px solid var(--border-muted, rgba(255, 255, 255, 0.12));
				border-radius: 6px;
				color: var(--text-1, #c2c9d1);
				font-size: 13px;
				padding: 7px 10px;
			}
			.se-footer {
				display: flex;
				align-items: center;
				gap: 10px;
				margin-top: 8px;
				padding-top: 16px;
				border-top: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.08));
			}
			.se-save-btn {
				display: inline-flex;
				align-items: center;
				gap: 6px;
				padding: 7px 16px;
				border-radius: 6px;
				font-size: 13px;
				font-weight: 500;
				cursor: pointer;
				border: none;
				background: var(--rose, #b8849e);
				color: #fff;
			}
		</style>
		<div class="se-page">
			<header class="se-header">
				<a href={resolve('/steering?tab=fleet')} class="se-back" aria-label="Back">←</a>
				<div class="se-header__titles">
					<h2 class="se-title">Memory Keeper</h2>
					<span class="se-slug">memory-keeper</span>
				</div>
			</header>
			<div class="se-body">
				<div class="se-banner se-banner--warn" role="status">
					<strong>Saved with warnings:</strong>
					<ul class="se-warning-list">
						<li>
							Steerer model "claude-opus-4-9999" could not be validated against the known model list
							— ensure the model name is correct before enabling in a production session.
						</li>
						<li>
							Confidence threshold 0.95 is unusually high; the steerer may fire infrequently.
							Consider a value in the 0.60–0.85 range for active sessions.
						</li>
						<li>
							No active sessions are currently enrolled — changes will take effect on the next
							enrolment.
						</li>
					</ul>
				</div>

				<fieldset class="se-fieldset">
					<legend class="se-legend">Configuration</legend>
					<div class="se-field">
						<label class="se-label" for="sw-model">Model</label>
						<input id="sw-model" type="text" class="se-input" value="claude-opus-4-9999" />
					</div>
					<div class="se-field">
						<label class="se-label" for="sw-ct">Confidence threshold</label>
						<input
							id="sw-ct"
							type="number"
							class="se-input"
							min="0"
							max="1"
							step="0.01"
							value="0.95"
						/>
					</div>
				</fieldset>

				<div class="se-footer">
					<button type="button" class="se-save-btn">💾 Save changes</button>
				</div>
			</div>
		</div>
	</div>
</Story>

<Story
	name="Loading"
	parameters={{
		docs: {
			description: {
				story: 'Skeleton loading state while the steerer is being fetched from /api/steerers/{id}.'
			}
		}
	}}
>
	<div class="sb-wrapper">
		<style>
			.sb-wrapper {
				background: var(--bg-base, #0f1117);
				min-height: 100vh;
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-sans, system-ui, sans-serif);
				font-size: 13px;
			}
			.se-page {
				max-width: 760px;
				margin: 0 auto;
				padding: 24px 20px 48px;
			}
			.se-header {
				display: flex;
				align-items: flex-start;
				gap: 10px;
				margin-bottom: 24px;
				border-bottom: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.08));
				padding-bottom: 16px;
			}
			.se-back {
				display: flex;
				align-items: center;
				justify-content: center;
				width: 28px;
				height: 28px;
				border-radius: 6px;
				border: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.08));
				color: var(--text-2, #adb5be);
				text-decoration: none;
				flex-shrink: 0;
				margin-top: 2px;
			}
			.se-title {
				margin: 0 0 2px;
				font-size: 16px;
				font-weight: 600;
				color: var(--text-strong, #dde2e8);
			}
			.se-body {
				display: flex;
				flex-direction: column;
				gap: 12px;
			}
			.se-skel-row {
				height: 20px;
				border-radius: 4px;
				background: linear-gradient(
					90deg,
					var(--bg-2, #1a1d26) 0%,
					var(--bg-3, #20232e) 50%,
					var(--bg-2, #1a1d26) 100%
				);
				background-size: 200% 100%;
				animation: skel-pulse 1.4s ease infinite;
			}
			.se-skel-row:nth-child(2) {
				width: 60%;
			}
			.se-skel-row:nth-child(3) {
				width: 80%;
			}
			.se-skel-row:nth-child(4) {
				width: 50%;
			}
			.se-skel-row:nth-child(5) {
				width: 70%;
			}
			@keyframes skel-pulse {
				0% {
					background-position: 200% 0;
				}
				100% {
					background-position: -200% 0;
				}
			}
		</style>
		<div class="se-page">
			<header class="se-header">
				<a href={resolve('/steering?tab=fleet')} class="se-back" aria-label="Back">←</a>
				<div>
					<h2 class="se-title">Loading…</h2>
				</div>
			</header>
			<div class="se-body" aria-busy="true" aria-label="Loading steerer">
				<div class="se-skel-row"></div>
				<div class="se-skel-row"></div>
				<div class="se-skel-row"></div>
				<div class="se-skel-row"></div>
				<div class="se-skel-row"></div>
			</div>
		</div>
	</div>
</Story>

<Story
	name="ErrorState"
	parameters={{
		docs: { description: { story: 'Failed to load the steerer — load error banner with retry.' } }
	}}
>
	<div class="sb-wrapper">
		<style>
			.sb-wrapper {
				background: var(--bg-base, #0f1117);
				min-height: 100vh;
				color: var(--text-1, #c2c9d1);
				font-family: var(--font-sans, system-ui, sans-serif);
				font-size: 13px;
			}
			.se-page {
				max-width: 760px;
				margin: 0 auto;
				padding: 24px 20px 48px;
			}
			.se-header {
				display: flex;
				align-items: flex-start;
				gap: 10px;
				margin-bottom: 24px;
				border-bottom: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.08));
				padding-bottom: 16px;
			}
			.se-back {
				display: flex;
				align-items: center;
				justify-content: center;
				width: 28px;
				height: 28px;
				border-radius: 6px;
				border: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.08));
				color: var(--text-2, #adb5be);
				text-decoration: none;
				flex-shrink: 0;
				margin-top: 2px;
			}
			.se-title {
				margin: 0 0 2px;
				font-size: 16px;
				font-weight: 600;
				color: var(--text-strong, #dde2e8);
			}
			.se-body {
				display: flex;
				flex-direction: column;
				gap: 16px;
			}
			.load-err {
				border: 1px solid var(--red-dim, rgba(199, 73, 73, 0.35));
				border-radius: 8px;
				background: rgba(199, 73, 73, 0.07);
				padding: 16px;
				display: flex;
				flex-direction: column;
				gap: 8px;
			}
			.load-err__title {
				font-weight: 600;
				color: var(--red, #c74949);
				font-size: 13px;
			}
			.load-err__detail {
				font-size: 12px;
				color: var(--text-2, #adb5be);
			}
			.load-err__retry {
				align-self: flex-start;
				padding: 5px 12px;
				border-radius: 6px;
				border: 1px solid var(--red-dim, rgba(199, 73, 73, 0.35));
				background: transparent;
				color: var(--red, #c74949);
				font-size: 12px;
				cursor: pointer;
			}
		</style>
		<div class="se-page">
			<header class="se-header">
				<a href={resolve('/steering?tab=fleet')} class="se-back" aria-label="Back">←</a>
				<div>
					<h2 class="se-title">Steerer</h2>
				</div>
			</header>
			<div class="se-body">
				<div class="load-err" role="alert">
					<div class="load-err__title">Couldn't load steerer</div>
					<div class="load-err__detail">Not found: no steerer with id 999</div>
					<button type="button" class="load-err__retry">Retry</button>
				</div>
			</div>
		</div>
	</div>
</Story>
