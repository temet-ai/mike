<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, userEvent, waitFor } from 'storybook/test';
	import SessionIdentityCard from './SessionIdentityCard.svelte';
	import { stubFetchJsonRouter, typed, type TypedBody } from '$lib/test-support/storyFetchStubs';
	import type { SessionIdentityDto } from './sessionIdentity';
	import { __resetApiCacheForTests } from '$lib/stores/cache/apiCache.svelte';
	import 'augmented-ui/augmented-ui.min.css';

	const SESSION_ID = '5d5b8d89-0000-4000-8000-000000000003';

	let identity: SessionIdentityDto = {
		goal: 'Implement session identity and notebook surfacing on the frontend.',
		next_steps: 'Wire the identity card into the fleet inspector and the comms page.',
		role: 'frontend-developer',
		name: 'frontend-dev-alpha',
		instructions:
			'Follow the FUNK style guide and CONVENTIONS.md for every visual and structural decision. Show inspection content in full.'
	};

	function router(method: string, url: string, body: unknown): TypedBody {
		if (method === 'PUT' && url.includes('/goal')) {
			const patch = body as Partial<typeof identity>;
			identity = { ...identity, ...patch };
			return typed<{ ok: boolean }>({ ok: true });
		}
		if (url.includes('/goal')) return typed<SessionIdentityDto>(identity);
		return typed<Record<string, never>>({});
	}

	const { Story } = defineMeta({
		title: 'Comms/SessionIdentityCard',
		component: SessionIdentityCard,
		tags: ['autodocs']
	});
</script>

<Story
	name="Editable"
	args={{ sessionId: SESSION_ID, fallbackLabel: 'session-5d5b8d89' }}
	loaders={[
		async () => {
			__resetApiCacheForTests();
			stubFetchJsonRouter(router);
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await waitFor(() => expect(canvas.getByText('frontend-dev-alpha')).toBeInTheDocument());
		await expect(canvas.getByText('frontend-developer')).toBeInTheDocument();
		await expect(
			canvas.getByText('Implement session identity and notebook surfacing on the frontend.')
		).toBeInTheDocument();

		const editBtn = canvas.getByRole('button', { name: 'Edit session identity' });
		await userEvent.click(editBtn);

		const nameInput = canvas.getByPlaceholderText('e.g. observer-1');
		await expect(nameInput).toHaveValue('frontend-dev-alpha');
		await userEvent.clear(nameInput);
		await userEvent.type(nameInput, 'frontend-dev-beta');

		const saveBtn = canvas.getByRole('button', { name: 'Save' });
		await userEvent.click(saveBtn);

		await waitFor(() => expect(canvas.getByText('frontend-dev-beta')).toBeInTheDocument());
		await expect(canvas.queryByRole('button', { name: 'Save' })).toBeNull();
	}}
/>
