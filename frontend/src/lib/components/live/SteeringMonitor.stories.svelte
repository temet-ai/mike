<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { resolve } from '$app/paths';
	import EmptyStateOnboarding from '$lib/components/shared/EmptyStateOnboarding.svelte';

	const { Story } = defineMeta({
		title: 'Routes/Steering/SteeringMonitor',
		component: EmptyStateOnboarding,
		tags: ['autodocs'],
		parameters: {
			layout: 'fullscreen',
			docs: {
				description: {
					component:
						'Visual spec for the live steering monitor panel. ' +
						'Static HTML — not the live component. ' +
						'See artifacts/2026-06-13/steering-plan-design.md §3.3.'
				}
			}
		}
	});
</script>

<Story name="Empty">
	{#snippet template()}
		<div class="min-h-screen bg-[var(--surface-bg)] p-6">
			<div class="sm-panel" style="max-width: 600px;">
				<div class="sm-header">
					<span class="sm-dot" aria-hidden="true"></span>
					<span class="sm-title">LIVE STEERING MONITOR</span>
					<span class="sm-enrolled-badge">0 sessions enrolled</span>
				</div>

				<div class="sm-awaiting">
					<p class="sm-await-text">Waiting for steering events…</p>
				</div>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="WsDown">
	{#snippet template()}
		<div class="min-h-screen bg-[var(--surface-bg)] p-6">
			<div class="sm-panel" style="max-width: 600px;">
				<div class="sm-ws-banner" role="alert">
					<span class="sm-banner-dot" aria-hidden="true"></span>
					Disconnected — live steering feed unavailable. Connect to the proxy to receive events.
				</div>

				<div class="sm-header">
					<span class="sm-dot" aria-hidden="true"></span>
					<span class="sm-title">LIVE STEERING MONITOR</span>
					<span class="sm-enrolled-badge">0 sessions enrolled</span>
				</div>

				<div class="sm-awaiting">
					<p class="sm-await-text">Waiting for connection…</p>
				</div>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="SingleSteererFeed">
	{#snippet template()}
		<div class="min-h-screen bg-[var(--surface-bg)] p-6">
			<div class="sm-panel" style="max-width: 680px;">
				<div class="sm-header">
					<span class="sm-dot" aria-hidden="true"></span>
					<span class="sm-title">LIVE STEERING MONITOR</span>
					<span class="sm-enrolled-badge">2 sessions enrolled</span>
				</div>

				<div class="sm-steerer-strip">
					<div class="sm-steerer-row">
						<span class="sm-steerer-name">Refocus</span>
						<span class="sm-steerer-stat sm-stat--fire">3 fires</span>
						<span class="sm-steerer-stat sm-stat--supp">1 suppressed</span>
						<span class="sm-steerer-stat sm-stat--time">last: 14:32:08</span>
					</div>
				</div>

				<div class="sm-feed" role="list" aria-label="Live steering interventions">
					<div class="sm-feed-row sm-feed-row--fired" role="listitem">
						<div
							class="sm-rail"
							style="background: var(--status-steering-fg);"
							aria-hidden="true"
						></div>
						<div class="sm-row-collapsed">
							<button
								type="button"
								class="sm-row-toggle"
								aria-expanded="false"
								aria-label="Toggle steering intervention detail"
							></button>
							<div class="sm-row-chrome">
								<span
									class="sm-chip"
									style="color: var(--status-success-fg); background: var(--status-success-bg); border-color: var(--status-success-border);"
								>
									<span
										class="sm-chip-dot"
										style="background: var(--status-success-fg);"
										aria-hidden="true"
									></span>
									fired
								</span>
								<span class="sm-steerer-label" style="color: var(--status-steering-fg);"
									>Refocus</span
								>
								<a href={resolve('sessions/abc123#steering')} class="sm-session-link">session →</a>
								<span class="sm-row-time">12s ago</span>
								<span class="sm-expand-cue" aria-hidden="true">▼</span>
							</div>
							<p class="sm-topic">
								The assistant pivoted to discussing implementation details of a Redis cache when the
								user asked about database indexing strategies. The off-thread drift score exceeded
								the drive threshold at 0.78.
							</p>
						</div>
					</div>

					<div class="sm-feed-row sm-feed-row--fired" role="listitem">
						<div
							class="sm-rail"
							style="background: var(--status-steering-fg);"
							aria-hidden="true"
						></div>
						<div class="sm-row-collapsed">
							<button
								type="button"
								class="sm-row-toggle"
								aria-expanded="false"
								aria-label="Toggle steering intervention detail"
							></button>
							<div class="sm-row-chrome">
								<span
									class="sm-chip"
									style="color: var(--status-success-fg); background: var(--status-success-bg); border-color: var(--status-success-border);"
								>
									<span
										class="sm-chip-dot"
										style="background: var(--status-success-fg);"
										aria-hidden="true"
									></span>
									fired
								</span>
								<span class="sm-steerer-label" style="color: var(--status-steering-fg);"
									>Refocus</span
								>
								<a href={resolve('sessions/def456#steering')} class="sm-session-link">session →</a>
								<span class="sm-row-time">3m ago</span>
								<span class="sm-expand-cue" aria-hidden="true">▼</span>
							</div>
						</div>
					</div>

					<div class="sm-feed-row sm-feed-row--fired" role="listitem">
						<div
							class="sm-rail"
							style="background: var(--status-steering-fg);"
							aria-hidden="true"
						></div>
						<div class="sm-row-collapsed">
							<button
								type="button"
								class="sm-row-toggle"
								aria-expanded="false"
								aria-label="Toggle steering intervention detail"
							></button>
							<div class="sm-row-chrome">
								<span
									class="sm-chip"
									style="color: var(--status-success-fg); background: var(--status-success-bg); border-color: var(--status-success-border);"
								>
									<span
										class="sm-chip-dot"
										style="background: var(--status-success-fg);"
										aria-hidden="true"
									></span>
									fired
								</span>
								<span class="sm-steerer-label" style="color: var(--status-steering-fg);"
									>Refocus</span
								>
								<a href={resolve('sessions/ghi789#steering')} class="sm-session-link">session →</a>
								<span class="sm-row-time">18m ago</span>
								<span class="sm-expand-cue" aria-hidden="true">▼</span>
							</div>
						</div>
					</div>

					<div class="sm-feed-row" role="listitem">
						<div class="sm-rail" style="background: var(--border-subtle);" aria-hidden="true"></div>
						<div class="sm-row-collapsed">
							<button
								type="button"
								class="sm-row-toggle"
								aria-expanded="false"
								aria-label="Toggle steering intervention detail"
							></button>
							<div class="sm-row-chrome">
								<span
									class="sm-chip"
									style="color: var(--text-2); background: var(--surface-2); border-color: var(--border-subtle);"
								>
									suppressed
								</span>
								<span class="sm-steerer-label" style="color: var(--text-2);">Refocus</span>
								<a href={resolve('sessions/abc123#steering')} class="sm-session-link">session →</a>
								<span class="sm-row-time">25m ago</span>
								<span class="sm-expand-cue" aria-hidden="true">▼</span>
							</div>
						</div>
					</div>
				</div>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="MultiSteererFeed">
	{#snippet template()}
		<div class="min-h-screen bg-[var(--surface-bg)] p-6">
			<div class="sm-panel" style="max-width: 680px;">
				<div class="sm-header">
					<span class="sm-dot" aria-hidden="true"></span>
					<span class="sm-title">LIVE STEERING MONITOR</span>
					<span class="sm-enrolled-badge">3 sessions enrolled</span>
				</div>

				<div class="sm-steerer-strip">
					<div class="sm-steerer-row">
						<span class="sm-steerer-name">Refocus</span>
						<span class="sm-steerer-stat sm-stat--fire">4 fires</span>
						<span class="sm-steerer-stat sm-stat--supp">2 suppressed</span>
						<span class="sm-steerer-stat sm-stat--time">last: 14:32:08</span>
					</div>
					<div class="sm-steerer-row">
						<span class="sm-steerer-name">MemoryKeeper</span>
						<span class="sm-steerer-stat sm-stat--fire">1 fire</span>
						<span class="sm-steerer-stat sm-stat--muted">3 no-drift</span>
						<span class="sm-steerer-stat sm-stat--time">last: 14:29:41</span>
					</div>
				</div>

				<div class="sm-feed" role="list" aria-label="Live steering interventions">
					<div class="sm-feed-row sm-feed-row--fired" role="listitem">
						<div
							class="sm-rail"
							style="background: var(--status-steering-fg);"
							aria-hidden="true"
						></div>
						<div class="sm-row-collapsed">
							<button
								type="button"
								class="sm-row-toggle"
								aria-expanded="false"
								aria-label="Toggle steering intervention detail"
							></button>
							<div class="sm-row-chrome">
								<span
									class="sm-chip"
									style="color: var(--status-success-fg); background: var(--status-success-bg); border-color: var(--status-success-border);"
								>
									<span
										class="sm-chip-dot"
										style="background: var(--status-success-fg);"
										aria-hidden="true"
									></span>
									fired
								</span>
								<span class="sm-steerer-label" style="color: var(--status-steering-fg);"
									>MemoryKeeper</span
								>
								<a href={resolve('sessions/abc123#steering')} class="sm-session-link">session →</a>
								<span class="sm-row-time">2m ago</span>
								<span class="sm-expand-cue" aria-hidden="true">▼</span>
							</div>
						</div>
					</div>

					<div class="sm-feed-row sm-feed-row--fired" role="listitem">
						<div
							class="sm-rail"
							style="background: var(--status-steering-fg);"
							aria-hidden="true"
						></div>
						<div class="sm-row-collapsed">
							<button
								type="button"
								class="sm-row-toggle"
								aria-expanded="false"
								aria-label="Toggle steering intervention detail"
							></button>
							<div class="sm-row-chrome">
								<span
									class="sm-chip"
									style="color: var(--status-success-fg); background: var(--status-success-bg); border-color: var(--status-success-border);"
								>
									<span
										class="sm-chip-dot"
										style="background: var(--status-success-fg);"
										aria-hidden="true"
									></span>
									fired
								</span>
								<span class="sm-steerer-label" style="color: var(--status-steering-fg);"
									>Refocus</span
								>
								<a href={resolve('sessions/def456#steering')} class="sm-session-link">session →</a>
								<span class="sm-row-time">5m ago</span>
								<span class="sm-expand-cue" aria-hidden="true">▼</span>
							</div>
							<p class="sm-topic">
								Drift detected toward general programming advice; steerer redirected to the original
								task of reviewing Postgres query plans.
							</p>
						</div>
					</div>

					<div class="sm-feed-row" role="listitem">
						<div class="sm-rail" style="background: var(--border-subtle);" aria-hidden="true"></div>
						<div class="sm-row-collapsed">
							<button
								type="button"
								class="sm-row-toggle"
								aria-expanded="false"
								aria-label="Toggle steering intervention detail"
							></button>
							<div class="sm-row-chrome">
								<span
									class="sm-chip"
									style="color: var(--text-2); background: var(--surface-2); border-color: var(--border-subtle);"
								>
									suppressed
								</span>
								<span class="sm-steerer-label" style="color: var(--text-2);">Refocus</span>
								<a href={resolve('sessions/ghi789#steering')} class="sm-session-link">session →</a>
								<span class="sm-row-time">12m ago</span>
								<span class="sm-expand-cue" aria-hidden="true">▼</span>
							</div>
						</div>
					</div>

					<div class="sm-feed-row" role="listitem">
						<div class="sm-rail" style="background: var(--border-subtle);" aria-hidden="true"></div>
						<div class="sm-row-collapsed">
							<button
								type="button"
								class="sm-row-toggle"
								aria-expanded="false"
								aria-label="Toggle steering intervention detail"
							></button>
							<div class="sm-row-chrome">
								<span
									class="sm-chip"
									style="color: var(--text-3); background: var(--surface-2); border-color: var(--border-subtle);"
								>
									no-drift
								</span>
								<span class="sm-steerer-label" style="color: var(--text-2);">Refocus</span>
								<a href={resolve('sessions/abc123#steering')} class="sm-session-link">session →</a>
								<span class="sm-row-time">18m ago</span>
								<span class="sm-expand-cue" aria-hidden="true">▼</span>
							</div>
						</div>
					</div>
				</div>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="ExpandedRow">
	{#snippet template()}
		<div class="min-h-screen bg-[var(--surface-bg)] p-6">
			<div class="sm-panel" style="max-width: 680px;">
				<div class="sm-header">
					<span class="sm-dot" aria-hidden="true"></span>
					<span class="sm-title">LIVE STEERING MONITOR</span>
					<span class="sm-enrolled-badge">1 session enrolled</span>
				</div>

				<div class="sm-steerer-strip">
					<div class="sm-steerer-row">
						<span class="sm-steerer-name">Refocus</span>
						<span class="sm-steerer-stat sm-stat--fire">1 fire</span>
						<span class="sm-steerer-stat sm-stat--time">last: 14:32:08</span>
					</div>
				</div>

				<div class="sm-feed" role="list" aria-label="Live steering interventions">
					<div class="sm-feed-row sm-feed-row--fired" role="listitem">
						<div
							class="sm-rail"
							style="background: var(--status-steering-fg);"
							aria-hidden="true"
						></div>

						<div class="sm-row-collapsed">
							<button
								type="button"
								class="sm-row-toggle"
								aria-expanded="true"
								aria-label="Toggle steering intervention detail"
							></button>
							<div class="sm-row-chrome">
								<span
									class="sm-chip"
									style="color: var(--status-success-fg); background: var(--status-success-bg); border-color: var(--status-success-border);"
								>
									<span
										class="sm-chip-dot"
										style="background: var(--status-success-fg);"
										aria-hidden="true"
									></span>
									fired
								</span>
								<span class="sm-steerer-label" style="color: var(--status-steering-fg);"
									>Refocus</span
								>
								<a href={resolve('sessions/abc123#steering')} class="sm-session-link">session →</a>
								<span class="sm-row-time">12s ago</span>
								<span class="sm-expand-cue" aria-hidden="true">▲</span>
							</div>
							<p class="sm-topic">
								The assistant started discussing React state management patterns when the session's
								focus is a Rust CLI tool. Drift score: 0.83 (threshold: 0.70). The following
								redirect payload was injected as a user turn.
							</p>
						</div>

						<div class="sm-row-body">
							<p class="sm-body-label">Injected payload</p>
							<pre
								class="sm-body-pre">Let's refocus. You were helping the user with a Rust CLI tool that parses JSON config files.

The current task is: implement the `--dry-run` flag that prints the parsed config without writing any files. The user's last message was asking about error handling for malformed JSON.

Please continue from where you left off with the Rust CLI implementation, specifically addressing the JSON parse error handling question.</pre>
							<a href={resolve('sessions/abc123#steering')} class="sm-full-record-link">
								→ Full record in session view
							</a>
						</div>
					</div>
				</div>
			</div>
		</div>
	{/snippet}
</Story>

<style>
	.sm-panel {
		display: flex;
		flex-direction: column;
		gap: 12px;
		height: 100%;
		min-height: 0;
	}

	.sm-ws-banner {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 12px;
		border: 1px solid var(--status-warning-border);
		background: var(--status-warning-bg);
		border-radius: 4px;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--status-warning-fg);
	}

	.sm-banner-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--status-warning-fg);
		animation: sm-pulse 2s cubic-bezier(0.4, 0, 0.6, 1) infinite;
		flex-shrink: 0;
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
		flex-shrink: 0;
	}

	.sm-title {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
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

	.sm-steerer-strip {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 8px 12px;
		background: var(--surface-2);
		border: 1px solid var(--border-subtle);
		border-radius: 4px;
	}

	.sm-steerer-row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
	}

	.sm-steerer-name {
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		font-weight: 600;
		color: var(--status-steering-fg);
		flex-shrink: 0;
	}

	.sm-steerer-stat {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		flex-shrink: 0;
	}

	.sm-stat--fire {
		color: var(--status-success-fg);
	}
	.sm-stat--supp {
		color: var(--text-2);
	}
	.sm-stat--muted {
		color: var(--text-3);
	}
	.sm-stat--time {
		color: var(--text-3);
		margin-left: auto;
	}

	.sm-awaiting {
		padding: 16px 0;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--text-3);
	}

	.sm-await-text {
		margin: 0;
	}

	.sm-feed {
		display: flex;
		flex-direction: column;
		gap: 4px;
		overflow-y: auto;
		flex: 1 1 auto;
		min-height: 0;
	}

	.sm-feed-row {
		position: relative;
		display: flex;
		flex-direction: column;
		border: 1px solid var(--border-subtle);
		border-radius: 6px;
		background: color-mix(in srgb, var(--surface-card) 60%, transparent 40%);
		overflow: hidden;
	}

	.sm-rail {
		position: absolute;
		inset-block: 4px;
		left: 0;
		width: 3px;
		border-radius: 0 2px 2px 0;
	}

	.sm-row-collapsed {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 8px 12px 8px 20px;
		text-align: left;
	}

	.sm-row-toggle {
		position: absolute;
		inset: 0;
		z-index: 0;
		margin: 0;
		padding: 0;
		background: transparent;
		border: none;
		cursor: pointer;
		width: 100%;
	}

	.sm-row-toggle:hover {
		background: color-mix(in srgb, var(--surface-2) 40%, transparent 60%);
	}

	.sm-row-toggle:focus-visible {
		outline: 1px solid var(--status-info-fg);
		outline-offset: -2px;
	}

	.sm-row-chrome {
		position: relative;
		z-index: 1;
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px;
		min-width: 0;
		pointer-events: none;
	}

	.sm-chip {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 1px 6px;
		border: 1px solid var(--border-subtle);
		border-radius: 4px;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		flex-shrink: 0;
	}

	.sm-chip-dot {
		display: inline-block;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		flex-shrink: 0;
	}

	.sm-steerer-label {
		font-family: var(--font-mono, monospace);
		font-size: 13px;
		font-weight: 500;
		flex-shrink: 0;
	}

	.sm-session-link {
		position: relative;
		z-index: 2;
		pointer-events: auto;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--status-info-fg);
		text-decoration: underline;
		text-decoration-color: var(--status-info-border);
		text-underline-offset: 2px;
		white-space: nowrap;
		flex-shrink: 0;
	}

	.sm-session-link:hover {
		color: var(--text-strong);
	}

	.sm-row-time {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
		margin-left: auto;
		flex-shrink: 0;
	}

	.sm-expand-cue {
		font-size: 10px;
		color: var(--text-faint);
		flex-shrink: 0;
	}

	.sm-topic {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 13px;
		color: var(--text-2);
		white-space: pre-wrap;
		overflow-wrap: break-word;
		word-break: break-word;
		text-align: left;
	}

	.sm-row-body {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 0 12px 10px 20px;
		border-top: 1px solid var(--border-subtle);
	}

	.sm-body-label {
		margin: 0;
		padding-top: 8px;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
	}

	.sm-body-pre {
		margin: 0;
		max-height: 240px;
		overflow-y: auto;
		padding: 10px;
		border: 1px solid var(--border-subtle);
		border-radius: 4px;
		background: var(--surface-2);
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--text-1);
		line-height: 1.5;
		white-space: pre-wrap;
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.sm-full-record-link {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--status-info-fg);
		text-decoration: underline;
		text-decoration-color: var(--status-info-border);
		text-underline-offset: 2px;
		align-self: flex-start;
	}

	.sm-full-record-link:hover {
		color: var(--text-strong);
	}

	@keyframes sm-pulse {
		0%,
		100% {
			opacity: 1;
		}
		50% {
			opacity: 0.4;
		}
	}
</style>
