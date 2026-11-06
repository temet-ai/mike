<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import EmptyStateOnboarding from '$lib/components/shared/EmptyStateOnboarding.svelte';

	const { Story } = defineMeta({
		title: 'Routes/Steering/Fleet',
		component: EmptyStateOnboarding,
		tags: ['autodocs'],
		parameters: {
			layout: 'fullscreen',
			docs: {
				description: {
					component:
						'Visual spec for the /steering?tab=fleet fleet management list page. ' +
						'Static HTML — not the live page. ' +
						'See docs/plans/scalable-noodling-squirrel.md §Phase D.'
				}
			}
		}
	});
</script>

<Story name="Empty">
	<div class="st-story-wrap">
		<div class="st-header">
			<div>
				<h2 class="st-title">Steerer Fleet</h2>
				<p class="st-subtitle">
					Registered steerers — manage the team, edit prompts, enrol sessions.
				</p>
			</div>
			<span class="st-btn-primary">+ New steerer</span>
		</div>
		<div class="st-empty">
			<p style="margin:0;font-size:13px;">No steerers registered yet.</p>
			<span class="st-btn-primary">+ Create first steerer</span>
		</div>
	</div>
</Story>

<Story name="Loaded">
	<div class="st-story-wrap">
		<div class="st-header">
			<div>
				<h2 class="st-title">Steerer Fleet</h2>
				<p class="st-subtitle">
					Registered steerers — manage the team, edit prompts, enrol sessions.
				</p>
			</div>
			<span class="st-btn-primary">+ New steerer</span>
		</div>
		<div class="st-table-wrap">
			<div class="st-thead">
				<span>On</span><span>Name</span><span>Abilities</span>
				<span>Model</span><span>Priority</span><span>Live activity</span>
			</div>
			<div class="st-row">
				<div>
					<span class="st-toggle-track st-toggle-track--on">
						<span class="st-toggle-thumb st-toggle-thumb--on"></span>
					</span>
				</div>
				<div style="display:flex;flex-direction:column;gap:2px;">
					<span class="st-name">Refocus</span>
					<span class="st-slug">refocus</span>
					<span class="st-desc">Redirects drifting sessions back to the current goal.</span>
				</div>
				<div style="display:flex;gap:4px;flex-wrap:wrap;">
					<span class="st-chip st-chip--supply">supply context</span>
					<span class="st-chip st-chip--augment">augment</span>
				</div>
				<div><span class="st-mono">openai/gpt-5.6-luna</span></div>
				<div><span class="st-mono">100</span></div>
				<div><span class="st-activity-idle">—</span></div>
			</div>
			<div class="st-row">
				<div>
					<span class="st-toggle-track st-toggle-track--on">
						<span class="st-toggle-thumb st-toggle-thumb--on"></span>
					</span>
				</div>
				<div style="display:flex;flex-direction:column;gap:2px;">
					<span class="st-name">Re-orienter</span>
					<span class="st-slug">re-orienter</span>
					<span class="st-desc">Redirects to a new goal when the session goes off-rails.</span>
				</div>
				<div style="display:flex;gap:4px;flex-wrap:wrap;">
					<span class="st-chip st-chip--redirect">redirect</span>
					<span class="st-chip st-chip--augment">augment</span>
				</div>
				<div><span class="st-mono">openai/gpt-5.4-mini</span></div>
				<div><span class="st-mono">200</span></div>
				<div><span class="st-activity-idle">—</span></div>
			</div>
			<div class="st-row st-row--disabled">
				<div>
					<span class="st-toggle-track">
						<span class="st-toggle-thumb"></span>
					</span>
				</div>
				<div style="display:flex;flex-direction:column;gap:2px;">
					<span class="st-name">Memory keeper</span>
					<span class="st-slug">memory-keeper</span>
					<span class="st-desc">Reminds the session of important context from earlier turns.</span>
				</div>
				<div style="display:flex;gap:4px;flex-wrap:wrap;">
					<span class="st-chip st-chip--supply">supply context</span>
					<span class="st-chip st-chip--augment">augment</span>
				</div>
				<div><span class="st-mono">openai/gpt-5.6-luna</span></div>
				<div><span class="st-mono">300</span></div>
				<div><span class="st-activity-idle">—</span></div>
			</div>
		</div>
	</div>
</Story>

<Story name="DangerBanner">
	<div class="st-story-wrap">
		<div class="st-header">
			<div>
				<h2 class="st-title">Steerer Fleet</h2>
				<p class="st-subtitle">
					Registered steerers — manage the team, edit prompts, enrol sessions.
				</p>
			</div>
			<span class="st-btn-primary">+ New steerer</span>
		</div>
		<div class="st-danger-banner">
			⚠ One or more <strong style="margin: 0 2px;">enabled</strong> steerers use
			<strong style="margin: 0 2px;">command authority</strong> or
			<strong style="margin: 0 2px;">drive mode</strong> — these can issue directives to the controlled
			session. Review before production use.
		</div>
		<div class="st-table-wrap">
			<div class="st-thead">
				<span>On</span><span>Name</span><span>Abilities</span>
				<span>Model</span><span>Priority</span><span>Live activity</span>
			</div>
			<div class="st-row">
				<div>
					<span class="st-toggle-track st-toggle-track--on">
						<span class="st-toggle-thumb st-toggle-thumb--on"></span>
					</span>
				</div>
				<div style="display:flex;flex-direction:column;gap:2px;">
					<span class="st-name">Hard Commander</span>
					<span class="st-slug">hard-commander</span>
					<span class="st-desc"
						>Experimental — issues binding commands to redirect session behaviour.</span
					>
				</div>
				<div style="display:flex;gap:4px;flex-wrap:wrap;">
					<span class="st-chip st-chip--command">⚠ command</span>
					<span class="st-chip st-chip--drive">⚡ drive</span>
				</div>
				<div><span class="st-mono">openai/gpt-5.4-mini</span></div>
				<div><span class="st-mono">50</span></div>
				<div><span class="st-activity-idle">—</span></div>
			</div>
		</div>
	</div>
</Story>

<Story name="LiveActivity">
	<div class="st-story-wrap">
		<div class="st-header">
			<div>
				<h2 class="st-title">Steerer Fleet</h2>
				<p class="st-subtitle">
					Registered steerers — manage the team, edit prompts, enrol sessions.
				</p>
			</div>
			<span class="st-btn-primary">+ New steerer</span>
		</div>
		<div class="st-table-wrap">
			<div class="st-thead">
				<span>On</span><span>Name</span><span>Abilities</span>
				<span>Model</span><span>Priority</span><span>Live activity</span>
			</div>
			<div class="st-row">
				<div>
					<span class="st-toggle-track st-toggle-track--on">
						<span class="st-toggle-thumb st-toggle-thumb--on"></span>
					</span>
				</div>
				<div style="display:flex;flex-direction:column;gap:2px;">
					<span class="st-name">Refocus</span>
					<span class="st-slug">refocus</span>
				</div>
				<div style="display:flex;gap:4px;flex-wrap:wrap;">
					<span class="st-chip st-chip--supply">supply context</span>
					<span class="st-chip st-chip--augment">augment</span>
				</div>
				<div><span class="st-mono">openai/gpt-5.6-luna</span></div>
				<div><span class="st-mono">100</span></div>
				<div style="display:flex;gap:6px;font-size:11px;">
					<span class="st-activity-fires">7 fired</span>
					<span style="color:var(--text-3,#c0c7d0);">2 supr</span>
				</div>
			</div>
			<div class="st-row">
				<div>
					<span class="st-toggle-track st-toggle-track--on">
						<span class="st-toggle-thumb st-toggle-thumb--on"></span>
					</span>
				</div>
				<div style="display:flex;flex-direction:column;gap:2px;">
					<span class="st-name">Re-orienter</span>
					<span class="st-slug">re-orienter</span>
				</div>
				<div style="display:flex;gap:4px;flex-wrap:wrap;">
					<span class="st-chip st-chip--redirect">redirect</span>
					<span class="st-chip st-chip--augment">augment</span>
				</div>
				<div><span class="st-mono">openai/gpt-5.4-mini</span></div>
				<div><span class="st-mono">200</span></div>
				<div style="display:flex;gap:6px;font-size:11px;">
					<span class="st-activity-fires">1 fired</span>
				</div>
			</div>
		</div>
	</div>
</Story>

<style>
	.st-story-wrap {
		background: var(--surface, #0d1117);
		min-height: 100vh;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-1, #c9d1d9);
	}

	.st-header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 12px;
		padding: 20px 24px 16px;
		border-bottom: 1px solid var(--border-color, #21262d);
	}

	.st-title {
		margin: 0;
		font-size: 1.1rem;
		font-weight: 600;
		color: var(--text-strong, #dde2e8);
	}

	.st-subtitle {
		margin: 4px 0 0;
		font-size: 12px;
		color: var(--text-3, #c0c7d0);
	}

	.st-btn-primary {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 6px 12px;
		border-radius: 4px;
		font-size: 12px;
		font-weight: 500;
		text-decoration: none;
		border: 1px solid var(--rose-border, #9c6b7e);
		cursor: pointer;
		color: var(--rose, #b8849e);
		background: var(--rose-dim, #2a1520);
	}

	.st-danger-banner {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		padding: 10px 24px;
		background: var(--amber-dim, #1c1400);
		border-bottom: 1px solid var(--amber-border, #9c7b3c);
		font-size: 12px;
		color: var(--text-1, #c9d1d9);
	}

	.st-table-wrap {
		padding: 0 24px 24px;
	}

	.st-thead {
		display: grid;
		grid-template-columns: 40px 1fr 180px 160px 64px 140px;
		gap: 0 12px;
		padding: 8px 12px;
		border-bottom: 1px solid var(--border-subtle, #161b22);
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3, #c0c7d0);
	}

	.st-row {
		display: grid;
		grid-template-columns: 40px 1fr 180px 160px 64px 140px;
		gap: 0 12px;
		padding: 10px 12px;
		border-bottom: 1px solid var(--border-subtle, #161b22);
		align-items: center;
	}

	.st-row--disabled {
		opacity: 0.6;
	}

	.st-name {
		font-size: 13px;
		font-weight: 500;
		color: var(--text-strong, #dde2e8);
	}
	.st-slug {
		font-size: 11px;
		color: var(--text-3, #c0c7d0);
	}
	.st-desc {
		font-size: 12px;
		color: var(--text-2, #adb5be);
		line-height: 1.4;
	}

	.st-toggle-track {
		display: inline-block;
		position: relative;
		width: 28px;
		height: 16px;
		border-radius: 8px;
		background: var(--surface-2, #161b22);
		border: 1px solid var(--border-subtle, #21262d);
	}

	.st-toggle-track--on {
		background: var(--rose-dim, #2a1520);
		border-color: var(--rose-border, #9c6b7e);
	}

	.st-toggle-thumb {
		display: block;
		position: absolute;
		top: 1px;
		left: 1px;
		width: 12px;
		height: 12px;
		border-radius: 50%;
		background: var(--text-3, #c0c7d0);
	}

	.st-toggle-thumb--on {
		transform: translateX(12px);
		background: var(--rose, #b8849e);
	}

	.st-chip {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		padding: 1px 6px;
		border: 1px solid var(--border-subtle, #21262d);
		border-radius: 4px;
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
	}

	.st-chip--supply {
		color: var(--text-2, #adb5be);
		background: var(--surface-2, #161b22);
	}
	.st-chip--redirect {
		color: var(--rose, #b8849e);
		background: var(--rose-dim, #2a1520);
		border-color: var(--rose-border, #9c6b7e);
	}
	.st-chip--command {
		color: var(--amber, #c49060);
		background: var(--amber-dim, #1c1400);
		border-color: var(--amber-border, #9c7b3c);
	}
	.st-chip--augment {
		color: var(--text-2, #adb5be);
		background: var(--surface-2, #161b22);
	}
	.st-chip--drive {
		color: var(--rose, #b8849e);
		background: var(--rose-dim, #2a1520);
		border-color: var(--rose-border, #9c6b7e);
	}

	.st-mono {
		font-size: 11px;
		color: var(--text-2, #adb5be);
	}
	.st-activity-fires {
		font-size: 11px;
		color: var(--green, #3fb950);
	}
	.st-activity-idle {
		font-size: 11px;
		color: var(--text-faint, #c0c7d0);
	}

	.st-empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 16px;
		padding: 64px 24px;
		color: var(--text-3, #c0c7d0);
	}
</style>
