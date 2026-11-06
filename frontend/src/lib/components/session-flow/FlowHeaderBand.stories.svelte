<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import FlowHeaderBand from './FlowHeaderBand.svelte';
	import { FLEET_PEER_ID, FLEET_SESSION_ID } from '$lib/session-flow/__fixtures__/fleet';

	const { Story } = defineMeta({
		title: 'Sessions/SessionFlow/FlowHeaderBand',
		component: FlowHeaderBand,
		tags: ['autodocs']
	});
</script>

<Story
	name="Identity"
	args={{
		sessionId: FLEET_SESSION_ID,
		peerId: FLEET_PEER_ID,
		startedAt: '2026-09-10T12:00:00Z',
		agentCount: 24,
		title: 'Fleet build — 24 delegated agents',
		timelineHref: `/sessions/${FLEET_SESSION_ID}?view=requests`
	}}
	play={async ({ canvas }) => {
		await expect(canvas.getByText('Session flow')).toBeVisible();
		await expect(canvas.getByText('Fleet build — 24 delegated agents')).toBeVisible();
		await expect(
			canvas.getByText(
				`${FLEET_SESSION_ID} + peer ${FLEET_PEER_ID} · 10 Sep 2026 · 24 DELEGATED AGENTS`
			)
		).toBeVisible();
		const link = canvas.getByRole('link', { name: 'Timeline ↗' });
		await expect(link).toHaveAttribute('href', `/sessions/${FLEET_SESSION_ID}?view=requests`);
		await expect(canvas.queryByRole('button', { name: 'Session flow' })).not.toBeInTheDocument();
	}}
/>
