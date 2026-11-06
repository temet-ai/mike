<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent } from 'storybook/test';
	import StatusIndicatorsHarness from './__harness__/StatusIndicatorsHarness.svelte';
	import { proxyEvents } from '$lib/ws.svelte';
	import { shellState } from '$lib/stores/shellState.svelte';
	import { buildStatsResponse } from '$lib/test-support';

	let connectSpy = fn();

	function armConnectSpy(): void {
		connectSpy = fn();
		proxyEvents.connect = connectSpy;
	}

	function slotText(canvasElement: HTMLElement, slot: string): string {
		return (canvasElement.querySelector(`[data-slot="${slot}"]`)?.textContent ?? '')
			.replace(/\s+/g, ' ')
			.trim();
	}

	const { Story } = defineMeta({
		title: 'Shell/StatusIndicators',
		component: StatusIndicatorsHarness,
		tags: ['autodocs']
	});
</script>

<Story
	name="Connected"
	loaders={[
		() => {
			proxyEvents.state = 'connected';
			shellState.commandPaletteOpen = false;
			armConnectSpy();
			return {};
		}
	]}
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		expect(slotText(canvasElement, 'connection-status')).toContain('Connected');
		expect(slotText(canvasElement, 'capture-status')).toContain('Captured · 0');
		expect(canvasElement.querySelector('[data-slot="db-status-trigger"]')).toBeNull();
		const palette = canvasElement.querySelector(
			'[data-slot="command-palette-hint"]'
		) as HTMLButtonElement | null;
		expect(palette).not.toBeNull();
		await userEvent.click(palette!);
		expect(shellState.commandPaletteOpen).toBe(true);
	}}
>
	{#snippet template()}
		<div class="bg-[var(--surface-bg)] p-4 flex items-center justify-end">
			<StatusIndicatorsHarness stats={buildStatsResponse()} />
		</div>
	{/snippet}
</Story>

<Story
	name="Reconnecting"
	loaders={[
		() => {
			proxyEvents.state = 'reconnecting';
			shellState.commandPaletteOpen = false;
			armConnectSpy();
			return {};
		}
	]}
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		expect(slotText(canvasElement, 'connection-status')).toContain('Reconnecting…');
		expect(canvasElement.querySelector('[data-slot="db-status-trigger"]')).toBeNull();
	}}
>
	{#snippet template()}
		<div class="bg-[var(--surface-bg)] p-4 flex items-center justify-end">
			<StatusIndicatorsHarness stats={buildStatsResponse()} />
		</div>
	{/snippet}
</Story>

<Story
	name="Disconnected"
	loaders={[
		() => {
			proxyEvents.state = 'disconnected';
			shellState.commandPaletteOpen = false;
			armConnectSpy();
			return {};
		}
	]}
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const chip = canvasElement.querySelector(
			'[data-slot="connection-status"]'
		) as HTMLButtonElement | null;
		expect(slotText(canvasElement, 'connection-status')).toContain('Disconnected');
		expect(chip?.classList.contains('status-chip--err')).toBe(true);
		await userEvent.click(chip!);
		expect(connectSpy).toHaveBeenCalledTimes(1);
	}}
>
	{#snippet template()}
		<div class="bg-[var(--surface-bg)] p-4 flex items-center justify-end">
			<StatusIndicatorsHarness stats={buildStatsResponse()} />
		</div>
	{/snippet}
</Story>

<Story
	name="WithCapture"
	loaders={[
		() => {
			proxyEvents.state = 'connected';
			shellState.commandPaletteOpen = false;
			armConnectSpy();
			return {};
		}
	]}
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		expect(slotText(canvasElement, 'capture-status')).toContain('1,247');
		expect(slotText(canvasElement, 'connection-status')).toContain('Connected');
	}}
>
	{#snippet template()}
		<div class="bg-[var(--surface-bg)] p-4 flex items-center justify-end">
			<StatusIndicatorsHarness stats={buildStatsResponse({ request_count: 1247 })} />
		</div>
	{/snippet}
</Story>

<Story
	name="DbDisabled"
	loaders={[
		() => {
			proxyEvents.state = 'connected';
			shellState.commandPaletteOpen = false;
			armConnectSpy();
			return {};
		}
	]}
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const db = canvasElement.querySelector(
			'[data-slot="db-status-trigger"]'
		) as HTMLButtonElement | null;
		expect(db).not.toBeNull();
		expect(db?.textContent).toContain('Persistence off');
	}}
>
	{#snippet template()}
		<div class="bg-[var(--surface-bg)] p-4 flex items-center justify-end">
			<StatusIndicatorsHarness stats={buildStatsResponse({ db_enabled: false })} />
		</div>
	{/snippet}
</Story>

<Story
	name="NullStats"
	loaders={[
		() => {
			proxyEvents.state = 'connected';
			shellState.commandPaletteOpen = false;
			armConnectSpy();
			return {};
		}
	]}
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		expect(slotText(canvasElement, 'capture-status')).toContain('Captured · 0');
		expect(canvasElement.querySelector('[data-slot="db-status-trigger"]')).toBeNull();
	}}
>
	{#snippet template()}
		<div class="bg-[var(--surface-bg)] p-4 flex items-center justify-end">
			<StatusIndicatorsHarness stats={null} />
		</div>
	{/snippet}
</Story>

<Story
	name="AllWarnings"
	loaders={[
		() => {
			proxyEvents.state = 'disconnected';
			shellState.commandPaletteOpen = false;
			armConnectSpy();
			return {};
		}
	]}
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		expect(slotText(canvasElement, 'connection-status')).toContain('Disconnected');
		expect(slotText(canvasElement, 'db-status-trigger')).toContain('Persistence off');
	}}
>
	{#snippet template()}
		<div class="bg-[var(--surface-bg)] p-4 flex items-center justify-end">
			<StatusIndicatorsHarness
				stats={buildStatsResponse({ request_count: 42, db_enabled: false })}
			/>
		</div>
	{/snippet}
</Story>
