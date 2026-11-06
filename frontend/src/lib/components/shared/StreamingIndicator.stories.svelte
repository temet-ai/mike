<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import StreamingIndicator from './StreamingIndicator.svelte';

	const { Story } = defineMeta({
		title: 'Shared/StreamingIndicator',
		component: StreamingIndicator,
		tags: ['autodocs'],
		parameters: { layout: 'padded' }
	});
</script>

<Story name="OrbitSpinner">
	{#snippet template()}
		<div class="si-stage">
			<div class="si-concept-header">
				OrbitSpinner — concentric rings tumbling in 3D
				<span class="si-concept-sub">
					2s rotation burst → 10s hold still → repeat. rotate3d() only, no opacity/scale animation.
				</span>
			</div>

			<div class="si-section">
				<div class="si-label">Band header (StatusPill streaming/pending)</div>
				<div class="si-band-mockup">
					<span class="si-band-toggle">#42 · Turn 3</span>
					<span class="si-band-model">claude-opus-4</span>
					<span class="si-orbit-pill">
						<StreamingIndicator label="Response streaming" />
						<span class="si-orbit-pill-text">recv</span>
					</span>
					<span class="si-band-time">2s ago</span>
				</div>
			</div>

			<div class="si-section">
				<div class="si-label">Session list (live indicator slot)</div>
				<div class="si-session-mockup">
					<StreamingIndicator label="Active — request in flight" />
					<span class="si-session-name">abc12345</span>
					<span class="si-session-ts">2s ago</span>
				</div>
				<div class="si-session-mockup">
					<span class="si-slot-empty"></span>
					<span class="si-session-name">def67890</span>
					<span class="si-session-ts">4m ago</span>
				</div>
			</div>

			<div class="si-section">
				<div class="si-label">In-card streaming note (TurnCard)</div>
				<div class="si-card-mockup">
					<div class="si-card-turn-label">Assistant</div>
					<div class="si-card-prose">turn content above</div>
					<div class="si-card-stream-row">
						<StreamingIndicator label="Response streaming" />
						<span class="si-card-stream-label">Receiving stream</span>
					</div>
				</div>
			</div>

			<div class="si-note">
				Single fixed design — 16px outer / 11px mid / 6px inner. Same component instance at every
				streaming site.<br />
				Rotation-only keyframe (rotate3d). Zero opacity or scale animation.
			</div>
		</div>
	{/snippet}
</Story>

<style>
	.si-stage {
		padding: 20px;
		background: var(--surface-bg);
		min-width: 480px;
	}

	.si-concept-header {
		font-family: 'JetBrains Mono', monospace;
		font-size: 0.75rem;
		font-weight: 600;
		color: var(--text-1);
		margin-bottom: 24px;
		letter-spacing: 0.02em;
	}
	.si-concept-sub {
		display: block;
		font-weight: 400;
		color: var(--text-3);
		margin-top: 3px;
		font-size: 0.6875rem;
	}

	.si-section {
		margin-bottom: 24px;
	}
	.si-label {
		font-family: 'JetBrains Mono', monospace;
		font-size: 0.625rem;
		color: var(--text-3);
		margin-bottom: 8px;
		letter-spacing: 0.04em;
		text-transform: uppercase;
	}
	.si-note {
		font-family: 'JetBrains Mono', monospace;
		font-size: 0.625rem;
		color: var(--text-3);
		margin-top: 8px;
		line-height: 1.7;
	}

	.si-band-mockup {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 10px;
		background: var(--surface-card);
		border: 1px solid var(--border-default);
		border-radius: 4px;
	}
	.si-band-toggle {
		font-family: 'JetBrains Mono', monospace;
		font-size: 0.6875rem;
		color: var(--text-2);
		font-weight: 500;
	}
	.si-band-model {
		font-family: 'JetBrains Mono', monospace;
		font-size: 0.625rem;
		color: var(--text-3);
		flex: 1;
	}
	.si-band-time {
		font-family: 'JetBrains Mono', monospace;
		font-size: 0.625rem;
		color: var(--text-3);
	}

	.si-orbit-pill {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 2px 7px;
		border-radius: 4px;
		background: var(--status-warning-bg);
		border: 1px solid var(--status-warning-border);
	}
	.si-orbit-pill-text {
		font-family: 'JetBrains Mono', monospace;
		font-size: 0.6875rem;
		font-weight: 500;
		color: var(--text-3);
		letter-spacing: 0.02em;
	}

	.si-session-mockup {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 5px 8px;
		background: var(--surface-card);
		border-bottom: 1px solid var(--border-subtle);
		width: 240px;
	}
	.si-slot-empty {
		display: inline-block;
		width: 16px;
		height: 16px;
		flex-shrink: 0;
	}
	.si-session-name {
		font-family: 'JetBrains Mono', monospace;
		font-size: 0.6875rem;
		color: var(--text-1);
		flex: 1;
	}
	.si-session-ts {
		font-family: 'JetBrains Mono', monospace;
		font-size: 0.625rem;
		color: var(--text-3);
	}

	.si-card-mockup {
		background: var(--surface-card);
		border: 1px solid var(--border-default);
		border-left: 2px solid var(--border-subtle);
		border-radius: 0 4px 4px 0;
		padding: 8px 12px;
		max-width: 480px;
	}
	.si-card-turn-label {
		font-family: 'JetBrains Mono', monospace;
		font-size: 0.625rem;
		color: var(--text-3);
		font-weight: 600;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		margin-bottom: 4px;
	}
	.si-card-prose {
		font-family: 'JetBrains Mono', monospace;
		font-size: 0.75rem;
		color: var(--text-3);
		font-style: italic;
		margin-bottom: 8px;
		padding-bottom: 8px;
		border-bottom: 1px solid var(--border-subtle);
	}
	.si-card-stream-row {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.si-card-stream-label {
		font-family: 'JetBrains Mono', monospace;
		font-size: 0.6875rem;
		color: var(--text-3);
	}
</style>
