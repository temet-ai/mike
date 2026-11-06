<script lang="ts">
	import { FastForward, Pause, Play, Radio, Rewind, SkipBack, SkipForward } from '@lucide/svelte';
	import { Button } from '$lib/components/ui/button';
	import Badge from '$lib/components/shared/Badge.svelte';
	import InstrumentReadout from '$lib/components/shared/InstrumentReadout.svelte';
	import { nextTransportStepMs, previousTransportStepMs } from '$lib/session-flow/transport';
	import type { FlowTimeScale } from '$lib/session-flow/timeScale';
	import type { FlowBounds, FlowPlayback, FlowSeekHandler } from '$lib/session-flow/types';

	let {
		playback,
		bounds,
		steps,
		scale,
		onSeek
	}: {
		playback: FlowPlayback;
		bounds: FlowBounds | null;
		steps: readonly number[];
		scale: FlowTimeScale;
		onSeek: FlowSeekHandler;
	} = $props();

	const SPEEDS = [1, 10, 30, 120] as const;

	const REWIND_LABEL = 'Rewind to previous turn';
	const FAST_FORWARD_LABEL = 'Fast-forward to next turn';
	const SKIP_TO_START_LABEL = 'Skip to start of loaded evidence';
	const SKIP_TO_END_LABEL = 'Skip to end of loaded evidence';
	const FOLLOW_LIVE_LABEL = 'Follow live';

	function handleRewind(): void {
		const target = previousTransportStepMs(steps, playback.atMs, bounds, scale, playback.speed);
		if (target !== null) onSeek(target, { keepPlaying: true });
	}

	function handleFastForward(): void {
		const target = nextTransportStepMs(steps, playback.atMs, bounds, scale, playback.speed);
		if (target !== null) onSeek(target, { keepPlaying: true });
	}

	function formatClockHMS(atMs: number | null): string {
		if (atMs === null) return '--:--:--';
		const d = new Date(atMs);
		const pad = (n: number) => String(n).padStart(2, '0');
		return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
	}

	const clock = $derived(formatClockHMS(playback.atMs));
	const playPauseLabel = $derived(playback.mode === 'playing' ? 'Pause replay' : 'Play replay');
</script>

<div class="flow-transport" aria-label="Flow playback controls">
	<div class="flow-transport__buttons">
		<Button
			variant="outline"
			size="icon"
			aria-label={SKIP_TO_START_LABEL}
			title={SKIP_TO_START_LABEL}
			disabled={!bounds}
			onclick={() => bounds && onSeek(bounds.startMs, { keepPlaying: true })}
		>
			<SkipBack aria-hidden="true" />
		</Button>
		<Button
			variant="outline"
			size="icon"
			aria-label={REWIND_LABEL}
			title={REWIND_LABEL}
			disabled={!bounds}
			onclick={handleRewind}
		>
			<Rewind aria-hidden="true" />
		</Button>
		<Button
			variant="default"
			size="icon"
			aria-label={playPauseLabel}
			title={playPauseLabel}
			disabled={!bounds}
			onclick={() => (playback.mode === 'playing' ? playback.pause() : playback.play())}
		>
			{#if playback.mode === 'playing'}
				<Pause aria-hidden="true" />
			{:else}
				<Play aria-hidden="true" />
			{/if}
		</Button>
		<Button
			variant="outline"
			size="icon"
			aria-label={FAST_FORWARD_LABEL}
			title={FAST_FORWARD_LABEL}
			disabled={!bounds}
			onclick={handleFastForward}
		>
			<FastForward aria-hidden="true" />
		</Button>
		<Button
			variant="outline"
			size="icon"
			aria-label={SKIP_TO_END_LABEL}
			title={SKIP_TO_END_LABEL}
			disabled={!bounds}
			onclick={() => bounds && onSeek(bounds.endMs, { keepPlaying: true })}
		>
			<SkipForward aria-hidden="true" />
		</Button>
		<Button
			variant="outline"
			size="icon"
			aria-label={FOLLOW_LIVE_LABEL}
			title={FOLLOW_LIVE_LABEL}
			aria-pressed={playback.mode === 'live'}
			onclick={() => playback.returnToLive()}
		>
			<Radio aria-hidden="true" />
		</Button>
	</div>

	<label class="flow-transport__speed">
		<span class="machine flow-transport__speed-label">SPEED</span>
		<select
			aria-label="Replay speed"
			value={playback.speed}
			onchange={(event) => playback.setSpeed(Number(event.currentTarget.value))}
		>
			{#each SPEEDS as speed (speed)}
				<option value={speed}>{speed}×</option>
			{/each}
		</select>
	</label>

	{#if playback.mode === 'live'}
		<span class="flow-transport__mode" data-tone="live">LIVE</span>
	{:else}
		<Badge variant="chip-active">REPLAY</Badge>
	{/if}

	<div class="flow-transport__clock">
		<InstrumentReadout label="PLAYHEAD" value={clock} />
	</div>
</div>

<style>
	.flow-transport {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: var(--space-3);
	}
	.flow-transport__buttons {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}
	.flow-transport__speed {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}
	.flow-transport__speed-label {
		font-size: 9px;
		color: var(--text-3);
	}
	.flow-transport__speed select {
		font-family: var(--font-mono);
		font-size: 12px;
		color: var(--text-1);
		background: var(--surface-2);
		border: var(--hairline) solid var(--border-default);
		padding: 2px 6px;
	}
	.flow-transport__speed select:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.flow-transport__mode {
		display: inline-flex;
		align-items: center;
		padding: 2px 7px;
		font-family: var(--font-mono);
		font-size: 11px;
		font-weight: 500;
		letter-spacing: var(--track-machine, 0.04em);
		text-transform: uppercase;
		border-radius: 4px;
	}
	.flow-transport__mode[data-tone='live'] {
		color: var(--status-steering-fg);
		background: var(--status-steering-bg);
		border: 1px solid var(--status-steering-border);
	}
	.flow-transport__clock {
		margin-left: auto;
	}
</style>
