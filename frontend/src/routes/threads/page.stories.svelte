<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import ThreadsPage from './+page.svelte';
	import type { ThreadsPageData } from './+page';
	import {
		THREAD_DETAIL,
		THREAD_LIST,
		THREAD_PROJECTS,
		THREAD_PROJECT_UUID
	} from '$lib/components/threads/threadFixtures';
	import type { ThreadDetail, ThreadIdentity } from '$lib/api/workThreads';
	import { stubFetchJsonRouter, typed, type TypedBody } from '$lib/test-support/storyFetchStubs';

	const DATA: ThreadsPageData = {
		projects: THREAD_PROJECTS,
		projectsError: null,
		project: THREAD_PROJECT_UUID,
		status: '',
		list: THREAD_LIST,
		listError: null,
		slug: 'checkout-rewrite',
		detail: THREAD_DETAIL,
		detailError: null
	};

	let sent: { method: string; url: string; body: unknown } | null = null;

	function router(method: string, url: string, body: unknown): TypedBody {
		if (method === 'PATCH') {
			sent = { method, url, body };
			return typed<ThreadIdentity>(THREAD_DETAIL.thread);
		}
		if (url.includes('/api/work-threads/')) return typed<ThreadDetail>(THREAD_DETAIL);
		return typed<typeof THREAD_LIST>(THREAD_LIST);
	}

	const { Story } = defineMeta({
		title: 'Routes/Threads',
		component: ThreadsPage,
		tags: ['autodocs'],
		parameters: { layout: 'fullscreen' },
		argTypes: { data: { control: false } }
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			sent = null;
			stubFetchJsonRouter(router);
			return {};
		}
	]}
	args={{ data: DATA }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByRole('heading', { level: 1, name: 'Threads' })).toBeInTheDocument();
		await expect(canvas.getByLabelText('Project')).toHaveValue(THREAD_PROJECT_UUID);
		await expect(
			canvas.getByRole('heading', { level: 2, name: 'Checkout rewrite' })
		).toBeInTheDocument();
		await expect(canvas.getByRole('link', { name: /Legacy import/ })).toBeInTheDocument();

		await userEvent.click(canvas.getByRole('button', { name: 'Edit' }));
		const title = canvas.getByLabelText('Title');
		await userEvent.clear(title);
		await userEvent.type(title, 'Checkout rewrite 2');
		await userEvent.click(canvas.getByRole('button', { name: 'Save details' }));
		await waitFor(() =>
			expect(sent).toEqual({
				method: 'PATCH',
				url: '/api/work-threads/checkout-rewrite',
				body: { project: THREAD_PROJECT_UUID, title: 'Checkout rewrite 2' }
			})
		);
	}}
/>
