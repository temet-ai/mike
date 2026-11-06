<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import FlowPlayerBand from './FlowPlayerBand.svelte';
	import { fleetDataset } from '$lib/session-flow/__fixtures__/fleet';
	import { buildTransportSteps } from '$lib/session-flow/transport';
	import type { FlowBounds, FlowSeekOptions } from '$lib/session-flow/types';
	import { createFlowPlayback } from '$lib/session-flow/playback.svelte';

	const { Story } = defineMeta({
		title: 'Sessions/SessionFlow/FlowPlayerBand',
		component: FlowPlayerBand,
		parameters: { layout: 'padded' }
	});
</script>

<script lang="ts">
	const dataset = fleetDataset({ coordinatorTurns: 12 });
	const bounds = dataset.bounds as FlowBounds;
	const playback = createFlowPlayback({ dataset });

	let brush = $state<FlowBounds | null>(null);
	let liveFrontierMs = $state<number | null>(null);

	function handleSeek(atMs: number, options?: FlowSeekOptions): void {
		playback.seek(atMs, options);
	}
	function handleBrush(range: FlowBounds | null): void {
		brush = range;
	}
</script>

<Story
	name="Timeline"
	play={async ({ canvas }) => {
		const transport = canvas.getByLabelText('Flow playback controls');
		const rectBefore = transport.getBoundingClientRect();
		const withinTransport = within(transport);
		const clockText = () => withinTransport.getByText(/^\d{2}:\d{2}:\d{2}$/).textContent;

		await userEvent.click(canvas.getByRole('button', { name: 'Skip to start of loaded evidence' }));
		await expect(playback.atMs).toBe(bounds.startMs);
		await expect(playback.mode).toBe('paused');

		const ffButton = canvas.getByRole('button', { name: 'Fast-forward to next turn' });
		const visited: number[] = [];
		for (let i = 0; i < 10; i += 1) {
			await userEvent.click(ffButton);
			visited.push(playback.atMs!);
		}
		const stepsAfterStart = buildTransportSteps(dataset).filter((step) => step > bounds.startMs);
		const expectedSteps: number[] = [];
		for (const step of stepsAfterStart) {
			if (expectedSteps.length >= 10) break;
			expectedSteps.push(step);
		}
		await expect(visited).toEqual(expectedSteps);
		await expect(new Set(visited).size).toBe(10);
		for (let i = 1; i < visited.length; i += 1)
			await expect(visited[i]).toBeGreaterThan(visited[i - 1]!);
		for (const atMs of visited) await expect(atMs).not.toBe(bounds.endMs);

		await userEvent.click(canvas.getByRole('button', { name: 'Skip to end of loaded evidence' }));
		await expect(playback.atMs).toBe(bounds.endMs);
		const clockAtEnd = clockText();
		await userEvent.click(canvas.getByRole('button', { name: 'Rewind to previous turn' }));
		await expect(playback.atMs).toBeLessThan(bounds.endMs);
		await expect(clockText()).not.toBe(clockAtEnd);

		await userEvent.click(canvas.getByRole('button', { name: 'Skip to start of loaded evidence' }));
		await userEvent.click(ffButton);
		await expect(playback.atMs).toBeLessThan(expectedSteps[expectedSteps.length - 1]!);

		await userEvent.click(canvas.getByRole('button', { name: 'Play replay' }));
		await expect(playback.mode).toBe('playing');
		await expect(canvas.getByRole('button', { name: 'Pause replay' })).toBeVisible();
		await expect(canvas.queryByText('Paused')).not.toBeInTheDocument();

		const rectDuringPlay = transport.getBoundingClientRect();
		await expect(rectDuringPlay.x).toBe(rectBefore.x);
		await expect(rectDuringPlay.y).toBe(rectBefore.y);
		await expect(rectDuringPlay.width).toBe(rectBefore.width);
		await expect(rectDuringPlay.height).toBe(rectBefore.height);

		await userEvent.click(canvas.getByRole('button', { name: 'Fast-forward to next turn' }));
		await expect(playback.mode).toBe('playing');
		await expect(canvas.getByRole('button', { name: 'Pause replay' })).toBeVisible();
		await expect(canvas.getByText('REPLAY')).toBeVisible();
		await userEvent.click(canvas.getByRole('button', { name: 'Rewind to previous turn' }));
		await expect(playback.mode).toBe('playing');

		await userEvent.click(canvas.getByRole('button', { name: 'Pause replay' }));
		await expect(playback.mode).toBe('paused');

		await userEvent.click(canvas.getByRole('button', { name: 'Play replay' }));
		await expect(playback.mode).toBe('playing');
		const markerButton = canvas.getAllByRole('button', { name: /Loaded activity start/ })[0]!;
		await userEvent.click(markerButton);
		await expect(playback.mode).toBe('paused');

		await userEvent.click(canvas.getByRole('button', { name: 'Skip to end of loaded evidence' }));
		await expect(playback.atMs).toBe(bounds.endMs);

		await expect(canvas.getByText('REPLAY')).toBeVisible();
		await expect(canvas.queryByText('LIVE')).not.toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div data-design="funk" style="background: var(--surface-bg); padding: 16px; max-width: 960px;">
			<FlowPlayerBand
				{dataset}
				{playback}
				{bounds}
				atMs={playback.atMs ?? bounds.startMs}
				{brush}
				{liveFrontierMs}
				onSeek={handleSeek}
				onBrush={handleBrush}
				width={900}
			/>
		</div>
	{/snippet}
</Story>
