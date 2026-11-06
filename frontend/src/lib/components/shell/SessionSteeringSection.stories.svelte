<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import 'augmented-ui/augmented-ui.min.css';

	const { Story } = defineMeta({
		title: 'Shell/SessionSteeringSection/Spec',
		tags: ['autodocs'],
		parameters: {
			docs: {
				description: {
					component:
						'Visual spec for the per-session steering section. ' +
						'Static HTML — not the live component. ' +
						'See artifacts/2026-06-13/steering-plan-design.md §Part 3. ' +
						'NOTE: Distillation artifacts are out of scope (separate future feature).'
				}
			}
		}
	});

	type VerdictCode =
		| 'fired'
		| 'suppressed_debounce'
		| 'suppressed_arbitration'
		| 'no_drift'
		| 'gate_miss'
		| 'failed_closed';

	type AuthorityCode = 'supply_context' | 'redirect' | 'command';
	type ModeCode = 'augment' | 'drive';

	interface Intervention {
		id: number;
		steererName: string;
		verdict: VerdictCode;
		authority: AuthorityCode;
		mode: ModeCode;
		offThreadTopic: string | null;
		confidence: number | null;
		time: string;
		sentRequestId: number | null;
	}

	function verdictChipStyle(v: VerdictCode): string {
		switch (v) {
			case 'fired':
				return 'color: var(--status-success-fg); background: var(--status-success-bg); border-color: var(--status-success-border);';
			case 'suppressed_debounce':
			case 'suppressed_arbitration':
				return 'color: var(--text-2); background: var(--surface-2); border-color: var(--border-subtle);';
			case 'no_drift':
				return 'color: var(--text-3); background: var(--surface-2); border-color: var(--border-subtle);';
			case 'gate_miss':
				return 'color: var(--text-3); background: transparent; border-color: transparent;';
			case 'failed_closed':
				return 'color: var(--status-danger-fg); background: var(--status-danger-bg); border-color: var(--status-danger-border);';
		}
	}

	function rowSurface(v: VerdictCode): string {
		switch (v) {
			case 'fired':
				return 'bg-[var(--surface-card)]/60';
			case 'suppressed_debounce':
			case 'suppressed_arbitration':
			case 'no_drift':
			case 'gate_miss':
			case 'failed_closed':
				return 'bg-[var(--surface-card)]/30';
		}
	}

	function authorityChipStyle(a: AuthorityCode): string {
		switch (a) {
			case 'command':
				return 'color: var(--status-warning-fg); background: var(--status-warning-bg); border-color: var(--status-warning-border);';
			case 'redirect':
				return 'color: var(--status-steering-fg); background: var(--status-steering-bg); border-color: var(--status-steering-border);';
			case 'supply_context':
				return 'color: var(--text-2); background: var(--surface-2); border-color: var(--border-subtle);';
		}
	}

	function modeChipStyle(m: ModeCode): string {
		return m === 'drive'
			? 'color: var(--status-steering-fg); background: var(--status-steering-bg); border-color: var(--status-steering-border);'
			: 'color: var(--text-2); background: var(--surface-2); border-color: var(--border-subtle);';
	}

	const FIRED_INTERVENTIONS: Intervention[] = [
		{
			id: 1,
			steererName: 'Refocus',
			verdict: 'fired',
			authority: 'redirect',
			mode: 'drive',
			offThreadTopic: 'Discussing Svelte component API outside of current thread scope.',
			confidence: 0.82,
			time: '08:13:42',
			sentRequestId: 42
		},
		{
			id: 2,
			steererName: 'Memory Keeper',
			verdict: 'fired',
			authority: 'supply_context',
			mode: 'augment',
			offThreadTopic: null,
			confidence: 0.76,
			time: '09:05:11',
			sentRequestId: 61
		},
		{
			id: 3,
			steererName: 'Refocus',
			verdict: 'fired',
			authority: 'command',
			mode: 'drive',
			offThreadTopic: 'Session proposed to skip the make check gate.',
			confidence: 0.95,
			time: '10:42:30',
			sentRequestId: 85
		}
	];

	const ALL_VERDICT_INTERVENTIONS: Intervention[] = [
		{
			id: 10,
			steererName: 'Refocus',
			verdict: 'fired',
			authority: 'redirect',
			mode: 'drive',
			offThreadTopic: 'Off-thread drift detected.',
			confidence: 0.82,
			time: '08:13:42',
			sentRequestId: 42
		},
		{
			id: 11,
			steererName: 'Refocus',
			verdict: 'suppressed_debounce',
			authority: 'redirect',
			mode: 'drive',
			offThreadTopic: 'Same off-thread topic as previous.',
			confidence: 0.79,
			time: '08:15:20',
			sentRequestId: null
		},
		{
			id: 12,
			steererName: 'Memory Keeper',
			verdict: 'suppressed_arbitration',
			authority: 'supply_context',
			mode: 'augment',
			offThreadTopic: null,
			confidence: 0.71,
			time: '08:15:22',
			sentRequestId: null
		},
		{
			id: 13,
			steererName: 'Refocus',
			verdict: 'no_drift',
			authority: 'redirect',
			mode: 'drive',
			offThreadTopic: null,
			confidence: 0.12,
			time: '08:20:05',
			sentRequestId: null
		},
		{
			id: 14,
			steererName: 'Refocus',
			verdict: 'gate_miss',
			authority: 'redirect',
			mode: 'drive',
			offThreadTopic: null,
			confidence: null,
			time: '08:22:10',
			sentRequestId: null
		},
		{
			id: 15,
			steererName: 'Refocus',
			verdict: 'failed_closed',
			authority: 'redirect',
			mode: 'drive',
			offThreadTopic: null,
			confidence: null,
			time: '08:30:00',
			sentRequestId: null
		}
	];
</script>

<Story name="Empty">
	{#snippet template()}
		<div class="min-h-screen bg-[var(--surface-bg)] p-6">
			<p class="font-mono text-caption text-[var(--text-3)] uppercase tracking-wider mb-4">
				Empty — no interventions; section does not render
			</p>
			<div
				class="font-mono text-caption text-[var(--text-2)] border border-dashed border-[var(--border-subtle)] rounded p-4 text-center"
			>
				[Steering section absent — no steering.interventions rows for this session]
			</div>
		</div>
	{/snippet}
</Story>

<Story name="FiresOnly">
	{#snippet template()}
		<div class="min-h-screen bg-[var(--surface-bg)] p-6">
			<p class="mb-4 font-mono text-caption text-[var(--text-3)] uppercase tracking-wider">
				3 fired interventions — all full opacity, teal rail accent
			</p>
			<div class="max-w-2xl space-y-1">
				<div class="mb-3 flex items-center gap-2">
					<span class="panel-subhd">Steering activity</span>
					<span class="font-mono text-caption text-[var(--text-2)]">3 interventions</span>
				</div>

				{#each FIRED_INTERVENTIONS as iv}
					<div
						class="relative flex min-h-0 flex-col rounded border border-[var(--border-subtle)] {rowSurface(
							iv.verdict
						)} pl-5 pr-3 py-2 gap-1"
					>
						<div
							class="absolute inset-y-[4px] left-0 w-[3px] rounded-sm"
							style="background: var(--status-steering-fg);"
							aria-hidden="true"
						></div>

						<div class="flex min-w-0 flex-wrap items-center gap-1.5">
							<span
								class="inline-flex items-center gap-1 rounded border px-1.5 py-px font-mono font-semibold uppercase"
								style="font-size: 11px; {verdictChipStyle(iv.verdict)}"
							>
								{iv.verdict}
							</span>
							<span class="font-mono text-sm font-medium text-[var(--status-steering-fg)]"
								>{iv.steererName}</span
							>
							<span class="ml-auto flex items-center gap-1">
								<span
									class="inline-flex items-center rounded border px-1 py-px font-mono font-semibold uppercase"
									style="font-size: var(--text-micro); {authorityChipStyle(iv.authority)}"
									>{iv.authority}</span
								>
								<span
									class="inline-flex items-center rounded border px-1 py-px font-mono font-semibold uppercase"
									style="font-size: var(--text-micro); {modeChipStyle(iv.mode)}">{iv.mode}</span
								>
								<span class="font-mono text-caption text-[var(--text-3)]">{iv.time}</span>
							</span>
						</div>
						{#if iv.offThreadTopic}
							<p class="font-mono text-sm text-[var(--text-2)] leading-snug">{iv.offThreadTopic}</p>
						{/if}
						{#if iv.sentRequestId}
							<p class="font-mono text-caption text-[var(--text-3)]">
								→ request <button
									type="button"
									class="text-[var(--text-2)] underline decoration-[var(--border-default)] underline-offset-1"
									>#{iv.sentRequestId}</button
								>
							</p>
						{/if}
					</div>
				{/each}
			</div>
		</div>
	{/snippet}
</Story>

<Story name="MixedTimeline">
	{#snippet template()}
		<div class="min-h-screen bg-[var(--surface-bg)] p-6">
			<p class="mb-4 font-mono text-caption text-[var(--text-3)] uppercase tracking-wider">
				Mixed timeline — fired (2) · suppressed (2) · no_drift (1) · gate_miss (1)
			</p>
			<div class="max-w-2xl space-y-1">
				<div class="mb-3 flex items-center gap-2">
					<span class="panel-subhd">Steering activity</span>
					<span class="font-mono text-caption text-[var(--text-2)]"
						>2 fired · 4 other evaluations</span
					>
				</div>

				{#each ALL_VERDICT_INTERVENTIONS as iv}
					<div
						class="relative flex min-h-0 flex-col rounded border border-[var(--border-subtle)] {rowSurface(
							iv.verdict
						)} pl-5 pr-3 py-2 gap-1"
					>
						<div
							class="absolute inset-y-[4px] left-0 w-[3px] rounded-sm"
							style="background: {iv.verdict === 'fired'
								? 'var(--status-steering-fg)'
								: 'var(--border-subtle)'};"
							aria-hidden="true"
						></div>

						<div class="flex min-w-0 flex-wrap items-center gap-1.5">
							<span
								class="inline-flex items-center gap-1 rounded border px-1.5 py-px font-mono font-semibold uppercase"
								style="font-size: 11px; {verdictChipStyle(iv.verdict)}"
							>
								{iv.verdict.replace('_', '​')}
							</span>
							<span
								class="font-mono text-sm font-medium"
								style="color: {iv.verdict === 'fired'
									? 'var(--status-steering-fg)'
									: 'var(--text-2)'}">{iv.steererName}</span
							>
							<span class="ml-auto flex items-center gap-1.5">
								{#if iv.confidence !== null}
									<span class="font-mono text-caption text-[var(--text-3)]"
										>{iv.confidence.toFixed(2)}</span
									>
								{/if}
								<span class="font-mono text-caption text-[var(--text-3)]">{iv.time}</span>
							</span>
						</div>
						{#if iv.offThreadTopic}
							<p class="font-mono text-sm text-[var(--text-2)] leading-snug">{iv.offThreadTopic}</p>
						{/if}
					</div>
				{/each}
			</div>
		</div>
	{/snippet}
</Story>

<Story name="AllVerdictTypes">
	{#snippet template()}
		<div class="min-h-screen bg-[var(--surface-bg)] p-6">
			<p class="mb-4 font-mono text-caption text-[var(--text-3)] uppercase tracking-wider">
				All verdict types — visual weight hierarchy
			</p>
			<div class="max-w-2xl space-y-1">
				{#each ALL_VERDICT_INTERVENTIONS as iv}
					<div
						class="relative flex min-h-0 flex-wrap items-center gap-2 rounded border border-[var(--border-subtle)] {rowSurface(
							iv.verdict
						)} pl-5 pr-3 py-2"
					>
						<div
							class="absolute inset-y-[4px] left-0 w-[3px] rounded-sm"
							style="background: {iv.verdict === 'fired'
								? 'var(--status-steering-fg)'
								: 'var(--border-subtle)'};"
							aria-hidden="true"
						></div>
						<span
							class="inline-flex items-center gap-1 rounded border px-1.5 py-px font-mono font-semibold uppercase"
							style="font-size: 11px; {verdictChipStyle(iv.verdict)}"
						>
							{iv.verdict.replace(/_/g, '​_')}
						</span>
						<span
							class="font-mono text-sm"
							style="color: {iv.verdict === 'fired'
								? 'var(--status-steering-fg)'
								: 'var(--text-2)'}">{iv.steererName}</span
						>
						{#if iv.confidence !== null}
							<span class="font-mono text-caption text-[var(--text-3)]"
								>conf {iv.confidence.toFixed(2)}</span
							>
						{/if}
						<span class="ml-auto font-mono text-caption text-[var(--text-3)]">{iv.time}</span>
					</div>
				{/each}
			</div>
			<p class="mt-4 font-mono text-caption text-[var(--text-2)]">
				Surface gradient: fired rows sit on the brighter surface; non-fired verdicts recede onto a
				dimmer surface tint. Text stays at full role colour — no opacity dimming. Only fired rows
				get the teal left rail accent.
			</p>
		</div>
	{/snippet}
</Story>

<Story name="LongContent">
	{#snippet template()}
		<div class="min-h-screen bg-[var(--surface-bg)] p-6">
			<p class="mb-1 font-mono text-caption text-[var(--text-3)] uppercase tracking-wider">
				Long content — no-truncation tripwire
			</p>
			<p class="mb-4 font-mono text-caption text-[var(--status-danger-fg)]">
				off_thread_topic must wrap. Zero line-clamp permitted. Row grows; section scrolls.
			</p>
			<div class="max-w-2xl space-y-1">
				<div
					class="relative flex min-h-0 flex-col rounded border border-[var(--border-subtle)] bg-[var(--surface-card)]/60 pl-5 pr-3 py-2 gap-2"
				>
					<div
						class="absolute inset-y-[4px] left-0 w-[3px] rounded-sm"
						style="background: var(--status-steering-fg);"
						aria-hidden="true"
					></div>
					<div class="flex min-w-0 items-center gap-1.5">
						<span
							class="inline-flex items-center gap-1 rounded border px-1.5 py-px font-mono font-semibold uppercase"
							style="font-size: 11px; {verdictChipStyle('fired')}"
						>
							fired
						</span>
						<span class="font-mono text-sm font-medium text-[var(--status-steering-fg)]"
							>Refocus</span
						>
						<span class="ml-auto font-mono text-caption text-[var(--text-3)]">08:13:42</span>
					</div>
					<p class="font-mono text-sm text-[var(--text-1)] leading-relaxed">
						The session has been discussing Svelte 5 runes component patterns and bits-ui Tabs API
						for the past three requests. None of this appears in the worklog Next list. The current
						thread objective is the steering migration SQL and the Rust capture-path wiring. The
						Svelte discussion is not blocked by the migration and should be deferred to a later
						session or a separate thread. The session has been off-thread for approximately 6
						requests (estimated 12 minutes of inference time).
					</p>
				</div>
			</div>
		</div>
	{/snippet}
</Story>
