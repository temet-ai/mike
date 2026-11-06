<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, waitFor } from 'storybook/test';
	import SessionLessonsPanel from './SessionLessonsPanel.svelte';

	const { Story } = defineMeta({
		title: 'Memory/SessionLessonsPanel',
		component: SessionLessonsPanel,
		tags: ['autodocs']
	});

	const SESSION_ID = '9d9b690e-cb59-4607-81f0-c96bdb3ff9b2';

	let calls = 0;
</script>

<script lang="ts">
	import { setContext } from 'svelte';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import { createCollapseStore } from '$lib/stores/collapseState.svelte';

	setContext(COLLAPSE_STORE_KEY, createCollapseStore());
</script>

<Story
	name="Error"
	args={{ sessionId: SESSION_ID }}
	loaders={[
		async () => {
			localStorage.clear();
			calls = 0;
			globalThis.fetch = (async () => {
				calls += 1;
				return new Response('down', { status: 503, statusText: 'Service Unavailable' });
			}) as typeof fetch;
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await userEvent.click(canvas.getByRole('button', { name: /lessons touching this session/i }));

		await waitFor(() =>
			expect(canvas.getByText("Couldn't load this session's lessons")).toBeInTheDocument()
		);
		await new Promise((resolve) => setTimeout(resolve, 1200));
		await expect(calls).toBe(1);

		await userEvent.click(canvas.getByRole('button', { name: /retry/i }));
		await waitFor(() => expect(calls).toBe(2));
		await new Promise((resolve) => setTimeout(resolve, 1200));
		await expect(calls).toBe(2);
		await expect(canvas.getByText("Couldn't load this session's lessons")).toBeInTheDocument();
	}}
>
	{#snippet template(args)}
		<div style="max-width: 820px; padding: 12px;">
			<SessionLessonsPanel {...args} />
		</div>
	{/snippet}
</Story>
