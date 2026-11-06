<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { resolve } from '$app/paths';
	import { ShipWheel } from '@lucide/svelte';
	import 'augmented-ui/augmented-ui.min.css';

	const { Story } = defineMeta({
		title: 'Components/Detail/BigbroAttribution',
		tags: ['autodocs'],
		parameters: {
			docs: {
				description: {
					component:
						'Visual spec for the BigbroAttribution card. ' +
						'Static HTML — not the live component. ' +
						'See artifacts/2026-06-13/steering-plan-design.md §Part 2.'
				}
			}
		}
	});

	const ANCHOR_LONG = `## State of play
Where we are: Implementing the steering persistence plan (steering-plan-db.md). The schema is designed but the migration SQL has not been written yet. We are in the design/planning phase following the draft ADR (docs/adr/0001-proxy-steered-session-grounding.md). The DB plan is complete and reviewed (artifacts/2026-06-13/steering-plan-db.md). The visual design spec is now in artifacts/2026-06-13/steering-plan-design.md following the design planning session.

Next: (1) write the SQL migration for the steering schema — one expand-only migration following the baseline pattern in 20260604000001_baseline.sql; (2) wire the Rust capture path — the turn-classifier in store/persistence/operator_turns.rs needs the steered branch; (3) back-fill interventions.sent_request_id once the capture row lands; (4) add LISTEN/NOTIFY triggers if the WS feed needs live-update on new interventions.

Files: docs/adr/0001-proxy-steered-session-grounding.md, artifacts/2026-06-13/steering-plan-db.md, artifacts/2026-06-13/steering-plan-design.md, docs/frontend/STYLE_GUIDE.md
Open questions: the session-close signal mechanism for the distiller — see steering-plan-db.md §What I need from rust-developer item 3. The embedding worker reference in the ADR may be inaccurate; rust-developer needs to confirm the actual close mechanism before we commit to a sessions.closed_at column vs reusing an existing queue.
Decisions: rose (#b8849e / --rose) is the colour family for steered turns; no new tokens needed (--rose-dim and rgba as border cover it); narrative_text preferred over summary_text to avoid the _summary fidelity tripwire; propose_only agent-definition artifacts get always-on amber left border.`;

	const OFF_THREAD_LONG =
		'The session has been discussing Svelte 5 runes component patterns and bits-ui Tabs API for the past three requests. ' +
		'None of this appears in the worklog Next list. The current thread objective is the steering migration SQL and the Rust capture-path wiring. ' +
		'The Svelte discussion is not blocked by the migration and should be deferred to a later session or a separate thread. ' +
		'The session has been off-thread for approximately 6 requests (estimated 12 minutes of inference time).';
</script>

<Story name="Default">
	{#snippet template()}
		<div class="min-h-screen bg-[var(--surface-bg)] p-6">
			<p class="mb-3 font-mono text-caption text-[var(--text-3)] uppercase tracking-wider">
				BigbroAttribution card — redirect authority · drive · confidence 0.82
			</p>

			<div
				class="card-surface rounded-lg border border-[var(--border-default)] bg-[var(--surface-card)] p-4 space-y-3 max-w-2xl"
			>
				<div class="flex items-center gap-2">
					<span
						class="inline-flex items-center gap-1.5 font-mono font-semibold uppercase tracking-kpi text-[var(--rose)]"
						style="font-size: var(--text-micro);"
					>
						<ShipWheel size={11} aria-hidden="true" />
						BIGBRO ATTRIBUTION
					</span>
				</div>

				<div>
					<span class="font-mono text-sm text-[var(--text-1)]">Refocus</span>
					<span class="ml-2 font-mono text-caption text-[var(--text-2)]"
						>google/gemini-3.5-flash-lite</span
					>
				</div>

				<div class="flex flex-wrap items-center gap-2">
					<span
						class="badge-nx inline-flex items-center rounded border px-1.5 py-px font-mono font-semibold uppercase"
						style="font-size: 11px; color: var(--rose); background: var(--rose-dim); border-color: var(--rose-border);"
					>
						redirect
					</span>
					<span
						class="badge-nx inline-flex items-center rounded border px-1.5 py-px font-mono font-semibold uppercase"
						style="font-size: 11px; color: var(--rose); background: var(--rose-dim); border-color: var(--rose-border);"
					>
						drive
					</span>
					<span
						class="badge-nx badge-active inline-flex items-center gap-1 rounded border px-1.5 py-px font-mono font-semibold uppercase"
						style="font-size: 11px;"
					>
						<span class="size-1.5 rounded-full bg-[var(--green)]" aria-hidden="true"></span>
						fired
					</span>
					<span class="font-mono text-caption text-[var(--text-2)]"
						>confidence <span class="text-[var(--text-1)]">0.82</span></span
					>
				</div>

				<div class="border-t border-[var(--border-subtle)]"></div>

				<div>
					<p
						class="mb-1 font-mono font-semibold uppercase tracking-kpi text-[var(--text-3)]"
						style="font-size: var(--text-micro);"
					>
						Off-thread topic
					</p>
					<p class="font-mono text-sm text-[var(--text-1)] leading-relaxed">{OFF_THREAD_LONG}</p>
				</div>

				<div>
					<div class="mb-1 flex items-center justify-between">
						<p
							class="font-mono font-semibold uppercase tracking-kpi text-[var(--text-3)]"
							style="font-size: var(--text-micro);"
						>
							Anchor at evaluation time
						</p>
						<button
							class="inline-flex items-center gap-1 rounded px-1.5 py-0.5 font-mono text-caption text-[var(--text-3)] hover:bg-[var(--surface-2)] hover:text-[var(--text-strong)]"
							aria-label="Copy anchor text"
						>
							copy
						</button>
					</div>
					<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
					<pre
						tabindex="0"
						role="region"
						aria-label="Anchor text — scrollable"
						class="overflow-y-auto rounded border border-[var(--border-subtle)] bg-[var(--surface-2)] p-3 font-mono text-sm text-[var(--text-1)] leading-relaxed whitespace-pre-wrap focus-visible:outline focus-visible:outline-1 focus-visible:-outline-offset-2 focus-visible:outline-[var(--interactive-focus-ring)]"
						style="max-height: 240px;">{ANCHOR_LONG}</pre>
				</div>

				<div class="border-t border-[var(--border-subtle)]"></div>

				<div class="flex flex-wrap items-center gap-4">
					<span class="font-mono text-caption text-[var(--text-2)]">
						Trigger: <a
							href={resolve('/sessions/f47ac10b-58cc-4372-a567-0e02b2c3d479/requests/41')}
							class="text-[var(--text-3)] underline decoration-[var(--border-default)] underline-offset-2 hover:text-[var(--text-1)]"
							>#41 →</a
						>
					</span>
					<span class="font-mono text-caption text-[var(--text-2)]">Sent: 2026-06-13 08:13:42Z</span
					>
					<span class="font-mono text-caption text-[var(--text-2)]"
						>Processed: 2026-06-13 08:13:48Z <span class="text-[var(--text-3)]">(6.1s)</span></span
					>
				</div>
			</div>
		</div>
	{/snippet}
</Story>
