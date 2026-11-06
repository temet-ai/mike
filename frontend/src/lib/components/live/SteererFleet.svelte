<script lang="ts">
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import { ApiError } from '$lib/api';
	import { listSteerers, patchSteerer } from '$lib/api/steering';
	import { onApiCacheUpdate } from '$lib/stores/cache/apiCache.svelte';
	import { createLiveSteeringStore } from '$lib/state/liveSteering.svelte';
	import type { LiveSteeringStore } from '$lib/state/liveSteering.svelte';
	import type { SteererDto } from '$lib/types';
	import { AUTHORITY_LABELS, STEERING_MODE_LABELS } from '$lib/steering';
	import type { Authority, SteeringMode } from '$lib/steering';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Plus, Bot, Zap, ShieldAlert } from '@lucide/svelte';

	interface Props {
		store?: LiveSteeringStore;
		onCount?: (n: number) => void;
	}
	let { store: storeProp, onCount }: Props = $props();

	let steerers = $state<SteererDto[]>([]);
	let error = $state<string | null>(null);
	let loading = $state(true);
	let togglingId = $state<number | null>(null);
	let rowErrors = $state<Map<number, string>>(new Map());

	// svelte-ignore state_referenced_locally
	const ownsLiveStore = storeProp === undefined;
	// svelte-ignore state_referenced_locally
	const liveStore = storeProp ?? createLiveSteeringStore();

	async function refresh(): Promise<void> {
		loading = true;
		error = null;
		try {
			steerers = await listSteerers();
		} catch (err) {
			steerers = [];
			error = err instanceof ApiError ? err.message : String(err);
		} finally {
			loading = false;
		}
	}

	async function toggleEnabled(s: SteererDto): Promise<void> {
		if (togglingId !== null) return;
		togglingId = s.id;

		const prev = s.enabled;
		steerers = steerers.map((r) => (r.id === s.id ? { ...r, enabled: !prev } : r));
		rowErrors = new Map([...rowErrors].filter(([k]) => k !== s.id));

		try {
			const envelope = await patchSteerer(s.id, { enabled: !prev });
			steerers = steerers.map((r) => (r.id === s.id ? { ...r, ...envelope.steerer } : r));
		} catch (err) {
			steerers = steerers.map((r) => (r.id === s.id ? { ...r, enabled: prev } : r));
			const msg = err instanceof ApiError ? err.message : String(err);
			rowErrors = new Map([...rowErrors, [s.id, msg]]);
		} finally {
			togglingId = null;
		}
	}

	onMount(() => {
		void refresh();
		const stop = ownsLiveStore ? liveStore.start() : null;
		const unsub = onApiCacheUpdate(
			'/api/steerers',
			() => {
				void refresh();
			},
			{ prefix: true }
		);
		return () => {
			unsub();
			stop?.();
		};
	});

	$effect(() => {
		if (!loading && error === null) onCount?.(steerers.length);
	});

	function liveCounters(name: string) {
		return liveStore.perSteerer.get(name) ?? null;
	}

	interface Accent {
		fg: string;
		bg: string;
		border: string;
	}

	function authorityAccent(a: Authority): Accent {
		switch (a) {
			case 'command':
				return {
					fg: 'var(--status-warning-fg)',
					bg: 'var(--status-warning-bg)',
					border: 'var(--status-warning-border)'
				};
			case 'redirect':
				return {
					fg: 'var(--status-danger-fg)',
					bg: 'var(--status-danger-bg)',
					border: 'var(--status-danger-border)'
				};
			default:
				return { fg: 'var(--text-1)', bg: 'var(--surface-2)', border: 'var(--border-subtle)' };
		}
	}

	function modeAccent(m: SteeringMode): Accent {
		return m === 'drive'
			? {
					fg: 'var(--status-danger-fg)',
					bg: 'var(--status-danger-bg)',
					border: 'var(--status-danger-border)'
				}
			: { fg: 'var(--text-1)', bg: 'var(--surface-2)', border: 'var(--border-subtle)' };
	}
</script>

<section
	class="st-page"
	data-slot="steerers-fleet"
	data-augmented-ui="tl-clip tr-clip br-clip bl-clip border"
>
	<header class="st-header">
		<div class="st-header__left">
			<span class="st-tag-backing">
				<h2 class="st-tag" data-augmented-ui="tl-clip br-clip border">Steerer Fleet</h2>
			</span>
			<p class="st-subtitle">
				Registered steerers — manage the team, edit prompts, enrol sessions.
			</p>
		</div>
		<div class="st-header__actions">
			<Button variant="outline" href={resolve('/steering/new')}>
				<Plus class="size-3.5" aria-hidden="true" />
				<span>New steerer</span>
			</Button>
		</div>
	</header>

	{#if loading && steerers.length === 0}
		<div class="st-skeleton" role="status" aria-busy="true" aria-label="Loading steerers">
			{#each { length: 3 } as _, i (i)}
				<div class="st-skel-row"></div>
			{/each}
		</div>
	{/if}

	{#if error}
		<div class="st-error-wrap">
			<LoadErrorBanner
				title="Couldn't load steerers"
				detail={error}
				hint="The /api/steerers endpoint may not be live yet."
				onRetry={refresh}
			/>
		</div>
	{/if}

	{#if !loading && !error && steerers.length === 0}
		<div class="st-empty">
			<div class="st-empty__icon" aria-hidden="true">
				<Bot size={32} />
			</div>
			<h3 class="st-empty__heading">No steerers registered yet</h3>
			<p class="st-empty__body">
				Steerers are named agents that observe or guide Claude sessions in real time. Create one to
				start shaping how your sessions behave.
			</p>
			<Button variant="outline" href={resolve('/steering/new')}>
				<Plus class="size-3.5" aria-hidden="true" />
				<span>Create your first steerer</span>
			</Button>
			<a href={resolve('/help/guides/steering-concepts')} class="st-empty__learn">
				How does steering work?
			</a>
		</div>
	{/if}

	{#if steerers.length > 0}
		<div class="st-table-wrap" aria-label="Steerer fleet">
			<div class="st-thead">
				<span class="st-th st-th--enabled">On</span>
				<span class="st-th st-th--name">Name</span>
				<span class="st-th st-th--abilities">Abilities</span>
				<span class="st-th st-th--scope">Scope</span>
				<span class="st-th st-th--model">Model</span>
				<span class="st-th st-th--priority">Priority</span>
				<span class="st-th st-th--enrolment">Enrolment</span>
				<span class="st-th st-th--activity">Live activity</span>
			</div>

			{#each steerers as s (s.id)}
				{@const live = liveCounters(s.name)}
				{@const authAccent = authorityAccent(s.authority)}
				{@const modeAcc = modeAccent(s.mode)}
				<div class="st-row" class:st-row--disabled={!s.enabled}>
					<div class="st-td st-td--enabled">
						<button
							type="button"
							class="st-toggle"
							class:st-toggle--on={s.enabled}
							aria-label="{s.enabled ? 'Disable' : 'Enable'} {s.display_name}"
							aria-pressed={s.enabled}
							disabled={togglingId === s.id}
							onclick={() => toggleEnabled(s)}
						>
							<span class="st-toggle__track" aria-hidden="true">
								<span class="st-toggle__thumb"></span>
							</span>
						</button>
					</div>

					<div class="st-td st-td--name">
						<a href={resolve(`/steering/${s.id}` as const)} class="st-link st-link--name">
							<span class="st-name">{s.display_name}</span>
							<span class="st-slug">{s.name}</span>
							{#if s.description}
								<span class="st-desc">{s.description}</span>
							{/if}
							{#if rowErrors.get(s.id)}
								<span class="st-row-error" role="alert">{rowErrors.get(s.id)}</span>
							{/if}
						</a>
					</div>

					<div class="st-td st-td--abilities">
						<span class="st-td-label">Abilities</span>
						<span class="st-chip-backing">
							<span
								class="st-chip"
								data-augmented-ui="tl-clip br-clip border"
								style="--acc-fg: {authAccent.fg}; --acc-bg: {authAccent.bg}; --aug-border-bg: {authAccent.border};"
							>
								{#if s.authority === 'command'}
									<ShieldAlert size={10} aria-hidden="true" />
								{/if}
								{AUTHORITY_LABELS[s.authority]}
							</span>
						</span>
						<span class="st-chip-backing">
							<span
								class="st-chip"
								data-augmented-ui="tl-clip br-clip border"
								style="--acc-fg: {modeAcc.fg}; --acc-bg: {modeAcc.bg}; --aug-border-bg: {modeAcc.border};"
							>
								{#if s.mode === 'drive'}
									<Zap size={10} aria-hidden="true" />
								{/if}
								{STEERING_MODE_LABELS[s.mode]}
							</span>
						</span>
					</div>

					<div class="st-td st-td--scope">
						<span class="st-td-label">Scope</span>
						<span class="st-mono">{s.project_path ?? 'Global'}</span>
					</div>

					<div class="st-td st-td--model">
						<span class="st-td-label">Model</span>
						<span class="st-mono">{s.model}</span>
					</div>

					<div class="st-td st-td--priority">
						<span class="st-td-label">Priority</span>
						<span class="st-mono">{s.priority}</span>
					</div>

					<div class="st-td st-td--enrolment">
						<span class="st-td-label">Enrolment</span>
						{#if (s.active_enrolment_count ?? 0) > 0}
							<span class="st-enrolment st-enrolment--active">
								{s.active_enrolment_count} active{#if !s.auto_enrol}&nbsp;<span
										class="st-enrolment-qualifier">· manual</span
									>{/if}
							</span>
						{:else if s.auto_enrol}
							<span class="st-enrolment st-enrolment--waiting">0 · auto-enrols</span>
						{:else}
							<span class="st-enrolment st-enrolment--unreached">0 · manual only</span>
						{/if}
					</div>

					<div class="st-td st-td--activity">
						<span class="st-td-label">Live activity</span>
						{#if live}
							<span class="st-activity">
								<span class="st-activity__fires">{live.fires} fired</span>
								{#if live.suppressions > 0}
									<span class="st-activity__supr">{live.suppressions} supr</span>
								{/if}
							</span>
						{:else}
							<span class="st-activity--idle">—</span>
						{/if}
					</div>
				</div>
			{/each}
		</div>
	{/if}
</section>

<style>
	.st-page {
		--edge: 14px;
		--aug-tl: var(--edge);
		--aug-tr: var(--edge);
		--aug-br: var(--edge);
		--aug-bl: var(--edge);
		--aug-border-all: 1.5px;
		--aug-border-bg: linear-gradient(
			90deg,
			var(--status-danger-fg) 0%,
			var(--status-danger-border) 26%,
			var(--border-default) 55%
		);
		padding: calc(var(--edge) + 4px) calc(var(--edge) + 8px) calc(var(--edge) + 6px)
			calc(var(--edge) + 4px);
		background:
			linear-gradient(105deg, var(--status-danger-bg), transparent 34%), var(--surface-bg);
		display: flex;
		flex-direction: column;
		gap: 12px;
		height: 100%;
		overflow: hidden;
	}

	.st-header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 12px;
		padding-bottom: 12px;
		border-bottom: 1px solid var(--border-default);
		flex-shrink: 0;
	}

	.st-header__left {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.st-header__actions {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-shrink: 0;
	}

	.st-tag-backing {
		align-self: flex-start;
		display: inline-flex;
		background: var(--surface-bg);
	}

	.st-tag {
		--aug-tl: 6px;
		--aug-tr: 0;
		--aug-br: 6px;
		--aug-bl: 0;
		--aug-border-all: 1px;
		--aug-border-bg: var(--status-danger-border);
		margin: 0;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 800;
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--status-danger-fg);
		padding: 3px 8px;
		background:
			linear-gradient(0deg, var(--status-danger-bg), var(--status-danger-bg)), var(--surface-bg);
	}

	.st-subtitle {
		margin: 0;
		font-size: var(--text-label);
		color: var(--text-3);
		font-family: var(--font-mono, monospace);
	}

	.st-skeleton {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.st-skel-row {
		height: 56px;
		background: linear-gradient(
			90deg,
			var(--surface-2) 0%,
			var(--surface-shine, var(--surface-2)) 50%,
			var(--surface-2) 100%
		);
		background-size: 200% 100%;
	}
	.st-skel-row {
		animation: st-shimmer 1.6s ease-in-out infinite;
	}

	@keyframes st-shimmer {
		0% {
			background-position: 200% 0;
		}
		100% {
			background-position: -200% 0;
		}
	}

	.st-empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 12px;
		padding: 56px 24px;
		text-align: center;
		max-width: 440px;
		margin: 0 auto;
	}

	.st-empty__icon {
		color: var(--text-3);
		margin-bottom: 4px;
	}

	.st-empty__heading {
		margin: 0;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 700;
		letter-spacing: -0.02em;
		font-size: 15px;
		color: var(--text-strong);
	}

	.st-empty__body {
		margin: 0;
		font-size: 13px;
		color: var(--text-2);
		line-height: 1.5;
		max-width: 360px;
	}

	.st-empty__learn {
		font-size: var(--text-xs, 12px);
		color: var(--text-3);
		text-decoration: underline;
		text-underline-offset: 2px;
		transition: color 0.15s;
	}

	.st-empty__learn:hover {
		color: var(--text-1);
	}

	.st-table-wrap {
		flex: 1;
		overflow-y: auto;
		container-type: inline-size;
		container-name: steerer-fleet;
	}

	.st-thead {
		display: grid;
		grid-template-columns: 40px 1fr 180px 110px 160px 64px 130px 140px;
		gap: 0 12px;
		padding: 8px 12px;
		position: sticky;
		top: 0;
		background: var(--surface-bg);
		border-bottom: 1px solid var(--border-subtle);
		z-index: 1;
	}

	.st-th {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
	}

	.st-row {
		display: grid;
		grid-template-columns: 40px 1fr 180px 110px 160px 64px 130px 140px;
		gap: 0 12px;
		padding: 10px 12px;
		border-bottom: 1px solid var(--border-subtle);
		align-items: center;
		transition: background 0.1s;
	}

	.st-row:hover {
		background: color-mix(in srgb, var(--surface-2) 40%, transparent 60%);
	}

	.st-row--disabled .st-toggle {
		opacity: 0.45;
	}

	.st-row--disabled .st-activity,
	.st-row--disabled .st-activity--idle {
		opacity: 0.55;
	}

	.st-td {
		display: flex;
		align-items: flex-start;
		min-width: 0;
	}

	.st-td--name {
		min-width: 0;
	}

	.st-td--abilities {
		flex-wrap: wrap;
		gap: 4px;
	}

	.st-td--model,
	.st-td--priority,
	.st-td--enrolment,
	.st-td--activity {
		align-items: center;
	}

	.st-enrolment {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-label);
	}
	.st-enrolment--active {
		color: var(--status-success-fg);
	}
	.st-enrolment--waiting {
		color: var(--text-3);
	}
	.st-enrolment--unreached {
		color: var(--status-warning-fg);
	}
	.st-enrolment-qualifier {
		color: var(--text-3);
		font-weight: 400;
	}

	.st-td--scope {
		align-items: center;
		overflow-wrap: anywhere;
	}

	.st-td-label {
		display: none;
	}

	.st-link {
		text-decoration: none;
		color: inherit;
	}

	.st-link--name {
		display: flex;
		flex-direction: column;
		gap: 2px;
		width: 100%;
	}

	.st-link--name:hover .st-name {
		color: var(--status-danger-fg);
		text-decoration: underline;
		text-underline-offset: 2px;
	}

	.st-name {
		font-size: 13px;
		font-weight: 500;
		color: var(--text-strong);
		line-height: 1.3;
	}

	.st-slug {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-label);
		color: var(--text-1);
	}

	.st-desc {
		font-size: var(--text-xs, 12px);
		color: var(--text-1);
		line-height: 1.4;
	}

	.st-row-error {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-label);
		color: var(--status-danger-fg);
		margin-top: 2px;
	}

	.st-toggle {
		background: none;
		border: none;
		cursor: pointer;
		padding: 2px;
	}

	.st-toggle:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.st-toggle__track {
		display: block;
		position: relative;
		width: 28px;
		height: 16px;
		border-radius: 8px;
		background: var(--surface-2);
		border: 1px solid var(--border-subtle);
		transition:
			background 0.15s,
			border-color 0.15s;
	}

	.st-toggle--on .st-toggle__track {
		background: var(--status-success-bg);
		border-color: var(--status-success-fg);
		box-shadow: 0 0 0 1px var(--status-success-border);
	}

	.st-toggle__thumb {
		display: block;
		position: absolute;
		top: 1px;
		left: 1px;
		width: 12px;
		height: 12px;
		border-radius: 50%;
		background: var(--text-3);
		transition:
			transform 0.15s,
			background 0.15s;
	}

	.st-toggle--on .st-toggle__thumb {
		transform: translateX(12px);
		background: var(--status-success-fg);
		box-shadow: 0 0 5px var(--status-success-border);
	}

	.st-chip-backing {
		display: inline-flex;
		background: var(--surface-bg);
	}

	.st-chip {
		--aug-tl: 5px;
		--aug-tr: 0;
		--aug-br: 5px;
		--aug-bl: 0;
		--aug-border-all: 1px;
		display: inline-flex;
		align-items: center;
		gap: 3px;
		padding: 1px 7px;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption);
		font-weight: 600;
		text-transform: uppercase;
		color: var(--acc-fg);
		background: linear-gradient(0deg, var(--acc-bg), var(--acc-bg)), var(--surface-bg);
		flex-shrink: 0;
	}

	.st-mono {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-label);
		color: var(--text-1);
	}

	.st-activity {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-label);
	}

	.st-activity__fires {
		color: var(--status-success-fg);
	}

	.st-activity__supr {
		color: var(--text-3);
	}

	.st-activity--idle {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-label);
		color: var(--text-faint);
	}

	@container steerer-fleet (max-width: 1099.98px) {
		.st-thead {
			display: none;
		}
		.st-row {
			grid-template-columns: 28px 1fr;
			grid-template-areas:
				'enabled name'
				'enabled abilities'
				'enabled scope'
				'enabled model'
				'enabled priority'
				'enabled enrolment'
				'enabled activity';
			row-gap: 6px;
			align-items: start;
		}
		.st-td--enabled {
			grid-area: enabled;
			padding-top: 2px;
		}
		.st-td--name {
			grid-area: name;
		}
		.st-td--abilities {
			grid-area: abilities;
			align-items: center;
		}
		.st-td--scope {
			grid-area: scope;
		}
		.st-td--model {
			grid-area: model;
		}
		.st-td--priority {
			grid-area: priority;
		}
		.st-td--enrolment {
			grid-area: enrolment;
		}
		.st-td--activity {
			grid-area: activity;
		}
		.st-td-label {
			display: inline-block;
			flex-shrink: 0;
			font-family: var(--font-mono, monospace);
			font-size: var(--text-caption);
			font-weight: 600;
			text-transform: uppercase;
			letter-spacing: 0.05em;
			color: var(--text-faint);
			margin-right: 6px;
		}
	}
</style>
