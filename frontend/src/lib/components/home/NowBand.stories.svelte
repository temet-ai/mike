<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import { buildMockSessionListStore } from '$lib/test-support/storyMocks';
	import type { SessionSummary } from '$lib/types';
	import NowBand from './NowBand.svelte';

	const NOW_MS = Date.now();
	const iso = (offsetMs: number): string => new Date(NOW_MS + offsetMs).toISOString();

	function session(overrides: Partial<SessionSummary> & { id: string }): SessionSummary {
		return {
			project: '/Users/dev/alpha',
			project_id: 1,
			cwd: '/Users/dev/alpha',
			project_resolution: 'git',
			first_seen: iso(-3_600_000),
			last_seen: iso(-30_000),
			request_count: 12,
			models: ['claude-opus-4-5'],
			last_status: 'Complete',
			title: null,
			...overrides
		};
	}

	const LIVE_ALPHA = session({
		id: '11111111-0000-0000-0000-000000000001',
		project: '/Users/dev/alpha',
		title: 'Implement auth middleware',
		last_status: 'Streaming',
		last_seen: iso(-15_000)
	});
	const LIVE_BETA = session({
		id: '22222222-0000-0000-0000-000000000001',
		project: '/Users/dev/beta',
		title: 'Refactor DB migrations',
		last_status: 'Pending',
		last_seen: iso(-45_000)
	});
	const HISTORY_ONLY = session({
		id: '33333333-0000-0000-0000-000000000001',
		project: '/Users/dev/alpha',
		title: 'Old finished session',
		last_status: 'Complete',
		last_seen: iso(-6 * 3_600_000)
	});

	const { Story } = defineMeta({
		title: 'Home/NowBand',
		component: NowBand,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	args={{
		sessionStore: buildMockSessionListStore({ sessions: [LIVE_ALPHA, LIVE_BETA, HISTORY_ONLY] }),
		now: NOW_MS
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getAllByText('Implement auth middleware').length).toBeGreaterThan(0);
		await expect(canvas.getAllByText('Refactor DB migrations').length).toBeGreaterThan(0);
		await expect(canvas.queryByText('Old finished session')).toBeNull();
		const titleLink = canvasElement.querySelector('.now-title') as HTMLAnchorElement | null;
		await expect(titleLink).not.toBeNull();
		await expect(titleLink).toHaveAttribute('href', '/sessions');
	}}
>
	{#snippet template(args)}
		<div class="w-[820px]">
			<NowBand {...args} />
		</div>
	{/snippet}
</Story>
