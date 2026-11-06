<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import HelpDrawerHarness from '../__harness__/HelpDrawerHarness.svelte';
	import { buildStatsResponse, stubFetchJson } from '$lib/test-support';
	import { buildDegradedCaptureHealth } from '$lib/test-support/healthFixtures';
	import type { HealthResponse } from '$lib/types';
	import { expect, within, userEvent } from 'storybook/test';

	const DEFAULT_STATS = buildStatsResponse({
		request_count: 42,
		active_streams: 1,
		uptime_seconds: 7320
	});

	async function failHealthFetch(): Promise<void> {
		const origFetch = globalThis.fetch;
		globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (url.includes('/api/health')) {
				throw new TypeError('Failed to fetch: inspect API unreachable');
			}
			return origFetch(input, init);
		}) as typeof fetch;
	}

	const { Story } = defineMeta({
		title: 'Shared/HelpDrawer',
		component: HelpDrawerHarness,
		tags: ['autodocs']
	});
</script>

<Story
	name="Open"
	args={{ stats: DEFAULT_STATS }}
	loaders={[
		async () => {
			stubFetchJson<HealthResponse>(buildDegradedCaptureHealth());
			return {};
		}
	]}
	play={async () => {
		const canvas = within(document.body);
		await expect(await canvas.findByText('reachable · degraded')).toBeVisible();
		await userEvent.click(canvas.getByText('Capture-gap diagnostics'));
		await expect(canvas.getByText(/Capture gap detected:/)).toBeVisible();
		await expect(canvas.getByText(/example-session-capture-gap/)).toBeVisible();
	}}
>
	{#snippet template(args)}
		<HelpDrawerHarness {...args} />
	{/snippet}
</Story>

<Story name="OpenNoStats" args={{ stats: null }}>
	{#snippet template(args)}
		<HelpDrawerHarness {...args} />
	{/snippet}
</Story>

<Story
	name="OpenLongUptime"
	args={{ stats: buildStatsResponse({ ...DEFAULT_STATS, uptime_seconds: 3 * 86400 + 5 * 3600 }) }}
>
	{#snippet template(args)}
		<HelpDrawerHarness {...args} />
	{/snippet}
</Story>

<Story
	name="OpenUnhealthy"
	args={{ stats: DEFAULT_STATS }}
	loaders={[
		async () => {
			await failHealthFetch();
			return {};
		}
	]}
>
	{#snippet template(args)}
		<HelpDrawerHarness {...args} />
	{/snippet}
</Story>
