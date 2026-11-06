<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect } from 'storybook/test';
	import CommsAvatar from './CommsAvatar.svelte';

	const SESSIONS = [
		{
			sessionUuid: 'db1-execute-triage-6f2a9c11',
			label: 'Execute triage map DB1 session',
			role: 'db-dev',
			agentType: null
		},
		{
			sessionUuid: 'fe-review-handover-7c3e0d42',
			label: 'Review session handover and merge verified work',
			role: 'frontend-developer',
			agentType: null
		},
		{
			sessionUuid: 'coordinator-9a1b5e60',
			label: 'Coordinator',
			role: null,
			agentType: 'coordinator'
		},
		{
			sessionUuid: 'a1b2c3d4-0000-4a11-9fee-unresolved00',
			label: null,
			role: null,
			agentType: null
		}
	];

	const { Story } = defineMeta({
		title: 'Comms/CommsAvatar',
		component: CommsAvatar,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(canvas.getAllByText('ET')).toHaveLength(2);
		await expect(canvas.getAllByText('RS')).toHaveLength(2);

		await expect(canvas.getAllByText('CO')).toHaveLength(2);

		await expect(canvas.getAllByText('A1')).toHaveLength(2);

		const decorative = canvasElement.querySelectorAll(
			'[data-slot="comms-avatar"][aria-hidden="true"]'
		);
		await expect(decorative.length).toBeGreaterThan(0);

		const standalone = canvasElement.querySelector('[data-slot="comms-avatar"][role="img"]');
		await expect(standalone).toHaveAttribute('aria-label', 'Coordinator');
	}}
>
	{#snippet template()}
		<div
			style="display: flex; flex-direction: column; gap: 16px; padding: 16px; background: var(--surface-bg);"
		>
			<div style="display: flex; align-items: center; gap: 12px;">
				{#each SESSIONS as session (session.sessionUuid)}
					<CommsAvatar
						{...session}
						size="message"
						decorative={session.label === 'Coordinator' ? false : true}
					/>
				{/each}
			</div>
			<div style="display: flex; align-items: center; gap: 8px;">
				{#each SESSIONS as session (session.sessionUuid)}
					<CommsAvatar {...session} size="compact" />
				{/each}
			</div>
		</div>
	{/snippet}
</Story>
