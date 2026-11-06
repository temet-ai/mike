<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';

	const { Story } = defineMeta({
		title: 'Shell/SteeringBand/Spec',
		tags: ['autodocs'],
		parameters: {
			docs: {
				description: {
					component:
						'Visual spec mockups for the steered turn-band treatment (6th turn type). ' +
						'Static HTML — not the live component. ' +
						'See artifacts/2026-06-13/steering-plan-design.md §Part 1 for the full spec.'
				}
			}
		}
	});

	const PAYLOAD_SHORT =
		'Stop. You have drifted off the thread. Resume worklog Next item (1): write the steering migration.';

	const PAYLOAD_LONG =
		'Stop. You have drifted off the current thread objective. ' +
		'The session is now discussing Svelte component API details that are not in the worklog Next list. ' +
		'Restate the current Next items: (1) write the steering migration SQL, (2) wire the Rust capture path. ' +
		'Do not start new work. Resume item (1) now.';

	const STEERED_BAND_CLASSES =
		'request-band turn-band turn-band--steered relative flex min-h-0 cursor-pointer ' +
		'flex-col rounded-md border border-[var(--border-subtle)] bg-[var(--surface-card)]/60 ' +
		'px-3.5 py-2.5 text-left transition-colors ' +
		'hover:border-[var(--border-default)] hover:bg-[var(--surface-2)] ' +
		'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--status-info-fg)]';

	const STEERED_BAND_CLASSES_RECESSIVE =
		'request-band turn-band turn-band--steered relative flex min-h-0 cursor-pointer ' +
		'flex-col rounded-md border border-[var(--border-subtle)] bg-[var(--surface-card)]/30 ' +
		'px-3.5 py-2.5 text-left transition-colors ' +
		'hover:border-[var(--border-default)] hover:bg-[var(--surface-2)] ' +
		'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--status-info-fg)]';
</script>

<Story name="Fired">
	{#snippet template()}
		<div class="min-h-screen bg-[var(--surface-bg)] p-6">
			<p class="mb-2 font-mono text-caption text-[var(--text-3)] uppercase tracking-wider">
				Steered turn — redirect authority · drive mode
			</p>
			<div
				class={STEERED_BAND_CLASSES}
				role="button"
				tabindex="0"
				aria-label="Request #42 — steered by Refocus (drive) — click to open detail"
			>
				<div
					class="absolute inset-y-[6px] left-0 w-[3px] rounded-sm bg-[var(--status-steering-fg)]"
					aria-hidden="true"
				></div>

				<div class="band-header mb-1.5 flex min-w-0 items-center gap-2">
					<span
						class="turn-chip turn-chip--steered inline-flex shrink-0 items-center gap-1 rounded-sm border px-1.5 py-px font-mono font-semibold uppercase tracking-kpi"
						style="font-size: var(--text-micro); color: var(--status-steering-fg); background: var(--status-steering-bg); border-color: var(--status-steering-border);"
						title="Steered by Refocus (drive · redirect)"
					>
						STEERED
					</span>

					<span
						class="shrink-0 font-mono text-label font-medium text-[var(--text-3)] underline decoration-[var(--border-default)] underline-offset-2"
					>
						#42
					</span>

					<span class="status-pill status-pill-complete shrink-0">complete</span>

					<span class="ml-auto shrink-0 font-mono text-caption text-[var(--status-steering-fg)]"
						>Refocus</span
					>

					<span class="shrink-0 font-mono text-caption text-[var(--text-2)]">08:13:42</span>
				</div>

				<p class="font-mono text-sm text-[var(--text-2)] leading-snug">
					{PAYLOAD_SHORT}
				</p>
			</div>

			<p class="mt-6 mb-2 font-mono text-caption text-[var(--text-3)] uppercase tracking-wider">
				For comparison — human turn (blue rail)
			</p>
			<div
				class="request-band turn-band relative flex min-h-0 flex-col rounded-md border border-[var(--border-subtle)] bg-[var(--surface-card)]/30 px-3.5 py-2.5"
				role="button"
				tabindex="0"
				aria-label="Request #41 — human turn"
			>
				<div
					class="absolute inset-y-[6px] left-0 w-[3px] rounded-sm bg-[var(--status-info-fg)]"
					aria-hidden="true"
				></div>
				<div class="flex min-w-0 items-center gap-2">
					<span
						class="shrink-0 rounded-sm border px-1.5 py-px font-mono font-semibold uppercase"
						style="font-size: var(--text-micro); color: var(--status-info-fg); background: var(--status-info-bg); border-color: var(--status-info-border);"
					>
						HUMAN
					</span>
					<span class="shrink-0 font-mono text-label font-medium text-[var(--text-3)]">#41</span>
					<span class="status-pill status-pill-complete shrink-0">complete</span>
					<span class="ml-auto font-mono text-caption text-[var(--text-2)]">08:13:10</span>
				</div>
				<p class="mt-1 font-mono text-sm text-[var(--text-2)]">
					Implement the steering migration and wire the capture path.
				</p>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="FiredCommand">
	{#snippet template()}
		<div class="min-h-screen bg-[var(--surface-bg)] p-6">
			<p class="mb-2 font-mono text-caption text-[var(--text-3)] uppercase tracking-wider">
				Command authority — chip is still STEERED (rose). Authority in tooltip only.
			</p>
			<div
				class={STEERED_BAND_CLASSES}
				role="button"
				tabindex="0"
				aria-label="Request #55 — steered by Refocus (drive · command)"
			>
				<div
					class="absolute inset-y-[6px] left-0 w-[3px] rounded-sm bg-[var(--status-steering-fg)]"
					aria-hidden="true"
				></div>
				<div class="band-header mb-1.5 flex min-w-0 items-center gap-2">
					<span
						class="shrink-0 inline-flex items-center rounded-sm border px-1.5 py-px font-mono font-semibold uppercase"
						style="font-size: var(--text-micro); color: var(--status-steering-fg); background: var(--status-steering-bg); border-color: var(--status-steering-border);"
						title="Steered by Refocus (drive · COMMAND — may cause tool execution)"
					>
						STEERED
					</span>
					<span class="shrink-0 font-mono text-label font-medium text-[var(--text-3)]">#55</span>
					<span class="status-pill status-pill-complete shrink-0">complete</span>
					<span class="ml-auto shrink-0 font-mono text-caption text-[var(--status-steering-fg)]"
						>Refocus</span
					>
					<span class="shrink-0 font-mono text-caption text-[var(--text-2)]">09:42:17</span>
				</div>
				<p class="font-mono text-sm text-[var(--text-2)] leading-snug">
					Run `cargo nextest run -p mike --test steering` before continuing.
				</p>
			</div>

			<p class="mt-4 font-mono text-caption text-[var(--text-2)]">
				The amber authority badge only appears in the expanded BigbroAttribution card and in the
				session steering section — not on the list row chip.
			</p>
		</div>
	{/snippet}
</Story>

<Story name="DriveVsAugment">
	{#snippet template()}
		<div class="min-h-screen bg-[var(--surface-bg)] p-6 space-y-4">
			<p class="font-mono text-caption text-[var(--text-3)] uppercase tracking-wider">
				Drive (acted independently) — steerer name brighter
			</p>
			<div
				class={STEERED_BAND_CLASSES}
				role="button"
				tabindex="0"
				aria-label="Request #60 — steered by Refocus (drive)"
			>
				<div
					class="absolute inset-y-[6px] left-0 w-[3px] rounded-sm bg-[var(--status-steering-fg)]"
					aria-hidden="true"
				></div>
				<div class="flex min-w-0 items-center gap-2">
					<span
						class="shrink-0 inline-flex items-center rounded-sm border px-1.5 py-px font-mono font-semibold uppercase"
						style="font-size: var(--text-micro); color: var(--status-steering-fg); background: var(--status-steering-bg); border-color: var(--status-steering-border);"
						title="Steered by Refocus (drive — acted without waiting for operator)"
					>
						STEERED
					</span>
					<span class="shrink-0 font-mono text-label font-medium text-[var(--text-3)]">#60</span>
					<span class="status-pill status-pill-complete shrink-0">complete</span>
					<span class="ml-auto shrink-0 font-mono text-caption text-[var(--status-steering-fg)]"
						>Refocus · drive</span
					>
					<span class="shrink-0 font-mono text-caption text-[var(--text-2)]">10:01:05</span>
				</div>
				<p class="mt-1.5 font-mono text-sm text-[var(--text-2)]">{PAYLOAD_SHORT}</p>
			</div>

			<p class="font-mono text-caption text-[var(--text-3)] uppercase tracking-wider">
				Augment (sent after operator turn) — steerer name muted
			</p>
			<div
				class={STEERED_BAND_CLASSES}
				role="button"
				tabindex="0"
				aria-label="Request #61 — steered by Memory Keeper (augment)"
			>
				<div
					class="absolute inset-y-[6px] left-0 w-[3px] rounded-sm bg-[var(--status-steering-fg)]"
					aria-hidden="true"
				></div>
				<div class="flex min-w-0 items-center gap-2">
					<span
						class="shrink-0 inline-flex items-center rounded-sm border px-1.5 py-px font-mono font-semibold uppercase"
						style="font-size: var(--text-micro); color: var(--status-steering-fg); background: var(--status-steering-bg); border-color: var(--status-steering-border);"
						title="Steered by Memory Keeper (augment — waited for operator turn)"
					>
						STEERED
					</span>
					<span class="shrink-0 font-mono text-label font-medium text-[var(--text-3)]">#61</span>
					<span class="status-pill status-pill-complete shrink-0">complete</span>
					<span class="ml-auto shrink-0 font-mono text-caption text-[var(--text-2)]"
						>Memory Keeper · augment</span
					>
					<span class="shrink-0 font-mono text-caption text-[var(--text-2)]">10:01:08</span>
				</div>
				<p class="mt-1.5 font-mono text-sm text-[var(--text-2)]">
					Adding context from the previous session: the steering migration is in
					`steering-plan-db.md`.
				</p>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="Reverted">
	{#snippet template()}
		<div class="min-h-screen bg-[var(--surface-bg)] p-6">
			<p class="mb-2 font-mono text-caption text-[var(--text-3)] uppercase tracking-wider">
				Reverted — operator marked this as a false positive
			</p>
			<div
				class={STEERED_BAND_CLASSES_RECESSIVE}
				role="button"
				tabindex="0"
				aria-label="Request #48 — steered by Refocus (reverted)"
			>
				<div
					class="absolute inset-y-[6px] left-0 w-[3px] rounded-sm bg-[var(--status-steering-fg)]"
					aria-hidden="true"
				></div>
				<div class="flex min-w-0 items-center gap-2">
					<span
						class="shrink-0 inline-flex items-center rounded-sm border px-1.5 py-px font-mono font-semibold uppercase"
						style="font-size: var(--text-micro); color: var(--status-steering-fg); background: var(--status-steering-bg); border-color: var(--status-steering-border);"
					>
						STEERED
					</span>
					<span
						class="shrink-0 inline-flex items-center rounded-sm border px-1.5 py-px font-mono font-semibold uppercase"
						style="font-size: var(--text-micro); color: var(--status-danger-fg); background: var(--status-danger-bg); border-color: var(--status-danger-border);"
					>
						REVERTED
					</span>
					<span class="shrink-0 font-mono text-label font-medium text-[var(--text-3)]">#48</span>
					<span class="status-pill status-pill-complete shrink-0">complete</span>
					<span class="ml-auto shrink-0 font-mono text-caption text-[var(--text-2)]">Refocus</span>
					<span class="shrink-0 font-mono text-caption text-[var(--text-2)]">09:05:30</span>
				</div>
				<p class="mt-1.5 font-mono text-sm text-[var(--text-2)] line-through">
					Stop. You have drifted — resume worklog Next item.
				</p>
				<div
					class="mt-2 rounded border border-[var(--status-danger-border)] bg-[var(--status-danger-bg)] px-2 py-1.5"
				>
					<span class="font-mono text-caption font-semibold text-[var(--status-danger-fg)]"
						>Revert reason:
					</span>
					<span class="font-mono text-caption text-[var(--text-1)]">
						False positive — the session was following a sub-task that IS in scope.
					</span>
				</div>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="PendingIdle">
	{#snippet template()}
		<div class="min-h-screen bg-[var(--surface-bg)] p-6">
			<p class="mb-2 font-mono text-caption text-[var(--text-3)] uppercase tracking-wider">
				Pending idle — sent but session not yet returned
			</p>
			<div
				class={STEERED_BAND_CLASSES}
				role="button"
				tabindex="0"
				aria-label="Request #77 — steered by Refocus (pending, awaiting idle session)"
			>
				<div
					class="absolute inset-y-[6px] left-0 w-[3px] rounded-sm bg-[var(--status-steering-fg)]"
					aria-hidden="true"
				></div>
				<div class="flex min-w-0 items-center gap-2">
					<span
						class="shrink-0 inline-flex items-center rounded-sm border px-1.5 py-px font-mono font-semibold uppercase"
						style="font-size: var(--text-micro); color: var(--status-steering-fg); background: var(--status-steering-bg); border-color: var(--status-steering-border);"
						title="Steered by Refocus (drive) — awaiting idle session return"
					>
						STEERED
					</span>
					<span
						class="shrink-0 inline-flex items-center gap-1 rounded border px-1.5 py-px font-mono font-semibold uppercase"
						style="font-size: var(--text-micro); color: var(--status-warning-fg); background: var(--status-warning-bg); border-color: var(--status-warning-border);"
					>
						<span class="size-1 rounded-full bg-[var(--status-warning-fg)]" aria-hidden="true"
						></span>
						pending
					</span>
					<span class="shrink-0 font-mono text-label font-medium text-[var(--text-3)]">#77</span>
					<span class="ml-auto shrink-0 font-mono text-caption text-[var(--status-steering-fg)]"
						>Refocus</span
					>
					<span class="shrink-0 font-mono text-caption text-[var(--text-2)]">11:30:00</span>
				</div>
				<p class="mt-1.5 font-mono text-sm text-[var(--text-2)]">
					{PAYLOAD_SHORT}
				</p>
				<p class="mt-1 font-mono text-caption text-[var(--text-3)]">
					Sent 11:30:00 · awaiting session return · expires 12:30:00
				</p>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="AllVariantsMatrix">
	{#snippet template()}
		<div class="min-h-screen bg-[var(--surface-bg)] p-6 space-y-3">
			<p class="font-mono text-caption text-[var(--text-3)] uppercase tracking-wider mb-4">
				All 6 authority × mode combinations — rose rail + STEERED chip on all
			</p>
			{#each [{ id: 80, authority: 'supply_context', mode: 'augment', name: 'Memory Keeper', payload: 'Context from previous session: the migration is in steering-plan-db.md §interventions.' }, { id: 81, authority: 'supply_context', mode: 'drive', name: 'Memory Keeper', payload: 'Context from previous session: the migration is in steering-plan-db.md §interventions.' }, { id: 82, authority: 'redirect', mode: 'augment', name: 'Re-orienter', payload: PAYLOAD_SHORT }, { id: 83, authority: 'redirect', mode: 'drive', name: 'Refocus', payload: PAYLOAD_SHORT }, { id: 84, authority: 'command', mode: 'augment', name: 'Refocus', payload: 'Run `make check` before claiming this task done.' }, { id: 85, authority: 'command', mode: 'drive', name: 'Refocus', payload: PAYLOAD_LONG }] as v}
				<div
					class={STEERED_BAND_CLASSES}
					role="button"
					tabindex="0"
					aria-label="Request #{v.id} — steered by {v.name} ({v.mode} · {v.authority})"
				>
					<div
						class="absolute inset-y-[6px] left-0 w-[3px] rounded-sm bg-[var(--status-steering-fg)]"
						aria-hidden="true"
					></div>
					<div class="flex min-w-0 items-center gap-2">
						<span
							class="shrink-0 inline-flex items-center rounded-sm border px-1.5 py-px font-mono font-semibold uppercase"
							style="font-size: var(--text-micro); color: var(--status-steering-fg); background: var(--status-steering-bg); border-color: var(--status-steering-border);"
							title="Steered by {v.name} ({v.mode} · {v.authority})"
						>
							STEERED
						</span>
						<span class="shrink-0 font-mono text-label font-medium text-[var(--text-3)]"
							>#{v.id}</span
						>
						<span class="status-pill status-pill-complete shrink-0">complete</span>
						<span
							class="ml-auto shrink-0 font-mono text-caption"
							style="color: var(--status-steering-fg)"
						>
							{v.name} · {v.authority} · {v.mode}
						</span>
					</div>
					<p class="mt-1.5 font-mono text-sm text-[var(--text-2)] leading-snug">{v.payload}</p>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>
