<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, waitFor } from 'storybook/test';
	import ConnectionBanner from './ConnectionBanner.svelte';
	import { proxyEvents } from '$lib/ws.svelte';

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
		title: 'Shell/ConnectionBanner',
		component: ConnectionBanner,
		tags: ['autodocs']
	});
</script>

<Story
	name="Hidden"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		expect(canvasElement.querySelector('[data-slot="connection-banner"]')).toBeNull();
		expect(canvasElement.textContent).toContain('Banner renders nothing when connected.');
	}}
>
	{#snippet template()}
		{@const _ = proxyEvents.state = 'connected'}
		{@const __ = armConnectSpy()}
		<div class="bg-[var(--surface-bg)] p-4 w-[600px]">
			<p class="text-xs text-[var(--text-2)] mb-2">Banner renders nothing when connected.</p>
			<ConnectionBanner visibilityDelayMs={0} />
			<p class="text-xs text-[var(--text-3)] mt-2">(no banner above)</p>
		</div>
	{/snippet}
</Story>

<Story
	name="Disconnected"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const retry = canvasElement.querySelector(
			'[data-slot="connection-banner-retry"]'
		) as HTMLButtonElement | null;
		const docs = canvasElement.querySelector(
			'[data-slot="connection-banner-docs"]'
		) as HTMLAnchorElement | null;
		await waitFor(() => {
			expect(slotText(canvasElement, 'connection-banner-title')).toBe(
				'Cannot reach this instance’s inspect API.'
			);
		});
		expect(retry).not.toBeNull();
		await userEvent.click(retry!);
		expect(connectSpy).toHaveBeenCalledTimes(1);
		expect(docs?.getAttribute('href')).toContain('/help/docs/recovery-and-maintenance');
		expect(docs?.hasAttribute('target')).toBe(false);
	}}
>
	{#snippet template()}
		{@const _ = proxyEvents.state = 'disconnected'}
		{@const __ = armConnectSpy()}
		<div class="bg-[var(--surface-bg)] w-[700px]">
			<ConnectionBanner visibilityDelayMs={0} />
		</div>
	{/snippet}
</Story>

<Story
	name="Reconnecting"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		await waitFor(() => {
			expect(slotText(canvasElement, 'connection-banner-title')).toBe(
				'Reconnecting to inspect API…'
			);
		});
		expect(canvasElement.querySelector('[data-slot="connection-banner-retry"]')).toBeNull();
		expect(canvasElement.querySelector('[data-slot="connection-banner-docs"]')).toBeNull();
	}}
>
	{#snippet template()}
		{@const _ = proxyEvents.state = 'reconnecting'}
		{@const __ = armConnectSpy()}
		<div class="bg-[var(--surface-bg)] w-[700px]">
			<ConnectionBanner visibilityDelayMs={0} />
		</div>
	{/snippet}
</Story>
