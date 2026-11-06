<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import RequestBodyCard from './RequestBodyCard.svelte';
	import 'augmented-ui/augmented-ui.min.css';
	import { __resetCacheForTests } from '$lib/stores/cache/unifiedCache.svelte';
	import { buildRequestDetail } from '$lib/components/__fixtures__/request';

	const { Story } = defineMeta({
		title: 'Components/Detail/RequestBodyCard',
		component: RequestBodyCard,
		tags: ['autodocs']
	});
</script>

<Story
	name="DeltaFastPath"
	args={{
		detail: buildRequestDetail({ id: 205, request_body_size: 500_000, message_count: 83 })
	}}
	loaders={[
		async () => {
			__resetCacheForTests();
			(globalThis as Record<string, unknown>).__fetchedUrls = [] as string[];
			const origFetch = globalThis.fetch;
			globalThis.fetch = (async (input: RequestInfo | URL) => {
				const url = typeof input === 'string' ? input : input.toString();
				((globalThis as Record<string, unknown>).__fetchedUrls as string[]).push(url);
				if (url.includes('/deltas')) {
					const payload = {
						request_id: 205,
						predecessor_request_id: 200,
						deltas: [
							{
								change_kind: 'added',
								ord: 82,
								message_id: 1,
								message_sha256: 'a',
								role: 'assistant',
								byte_size: 40,
								body: { role: 'assistant', content: 'ADDED_ASSISTANT_REPLY' }
							},
							{
								change_kind: 'added',
								ord: 81,
								message_id: 2,
								message_sha256: 'b',
								role: 'system',
								byte_size: 40,
								body: { role: 'system', content: 'ADDED_SYSTEM_REMINDER' }
							},
							{
								change_kind: 'removed',
								ord: 0,
								message_id: 3,
								message_sha256: 'c',
								role: 'user',
								byte_size: 30,
								body: { role: 'user', content: 'DROPPED_TURN' },
								removed_reason: 'compaction'
							}
						]
					};
					return new Response(JSON.stringify(payload), {
						status: 200,
						headers: { 'content-type': 'application/json' }
					});
				}
				if (url.includes('/body/request_sent')) {
					return new Response(
						JSON.stringify({
							text: '{"messages":[{"role":"user","content":"SENT_BODY_MARKER"}]}',
							byte_size: 58,
							has_body: true
						}),
						{ status: 200, headers: { 'content-type': 'application/json' } }
					);
				}
				if (url.includes('/body/request')) {
					return new Response(
						JSON.stringify({
							text: '{"messages":[{"role":"user","content":"ORIGINAL_FULL_BODY_MARKER"}]}',
							byte_size: 62,
							has_body: true
						}),
						{ status: 200, headers: { 'content-type': 'application/json' } }
					);
				}
				if (url.includes('/manifest')) {
					return new Response(
						JSON.stringify({ request_id: 205, has_manifest: false, manifest: null }),
						{ status: 200, headers: { 'content-type': 'application/json' } }
					);
				}
				return origFetch(input);
			}) as typeof fetch;
			return {
				teardown() {
					globalThis.fetch = origFetch;
				}
			};
		}
	]}
	play={async ({ canvasElement }) => {
		const { within, expect, userEvent, waitFor } = await import('storybook/test');
		const canvas = within(canvasElement);
		await waitFor(() => expect(canvas.getByText(/ADDED_SYSTEM_REMINDER/)).toBeInTheDocument());
		await expect(canvas.getByText(/ADDED_ASSISTANT_REPLY/)).toBeInTheDocument();
		await expect(canvas.getByText(/removed this turn/i)).toBeInTheDocument();
		const urls = (globalThis as Record<string, unknown>).__fetchedUrls as string[];
		await expect(urls.some((u) => u.includes('/deltas'))).toBe(true);
		await expect(
			urls.some((u) => u.includes('/body/request') && !u.includes('/body/request_sent'))
		).toBe(false);

		await userEvent.click(canvas.getByRole('tab', { name: 'Sent' }));
		const sentPanel = canvasElement.querySelector(
			'[data-slot="request-body-tab-sent"]'
		) as HTMLElement;
		await expect(sentPanel).toBeTruthy();
		await waitFor(() =>
			expect(within(sentPanel).getByText(/SENT_BODY_MARKER/)).toBeInTheDocument()
		);

		await userEvent.click(canvas.getByRole('tab', { name: 'Transformations' }));
		const transformationsPanel = canvasElement.querySelector(
			'[data-slot="request-body-tab-transformations"]'
		) as HTMLElement;
		await expect(transformationsPanel).toBeTruthy();
		await waitFor(() =>
			expect(
				within(transformationsPanel).getByText(
					/No transformation manifest recorded for this request/
				)
			).toBeInTheDocument()
		);

		await expect(
			urls.some((u) => u.includes('/body/request') && !u.includes('/body/request_sent'))
		).toBe(false);
		await userEvent.click(canvas.getByRole('tab', { name: 'Diff' }));
		const diffPanel = canvasElement.querySelector(
			'[data-slot="request-body-tab-diff"]'
		) as HTMLElement;
		await expect(diffPanel).toBeTruthy();
		await waitFor(() =>
			expect(within(diffPanel).getByText(/ORIGINAL_FULL_BODY_MARKER/)).toBeInTheDocument()
		);
		await expect(within(diffPanel).getByText(/SENT_BODY_MARKER/)).toBeInTheDocument();
		await expect(
			urls.some((u) => u.includes('/body/request') && !u.includes('/body/request_sent'))
		).toBe(true);
	}}
>
	{#snippet template(args)}
		<RequestBodyCard {...args} />
	{/snippet}
</Story>

<Story
	name="RebuiltBody"
	args={{
		detail: buildRequestDetail({ id: 206, request_body_size: 41_000, message_count: 1 })
	}}
	loaders={[
		async () => {
			__resetCacheForTests();
			const origFetch = globalThis.fetch;
			globalThis.fetch = (async (input: RequestInfo | URL) => {
				const url = typeof input === 'string' ? input : input.toString();
				if (url.includes('/deltas')) {
					return new Response(
						JSON.stringify({ request_id: 206, predecessor_request_id: null, deltas: [] }),
						{ status: 200, headers: { 'content-type': 'application/json' } }
					);
				}
				if (url.includes('/body/request_sent')) {
					return new Response(JSON.stringify({ text: null, byte_size: 0, has_body: false }), {
						status: 200,
						headers: { 'content-type': 'application/json' }
					});
				}
				if (url.includes('/body/request')) {
					return new Response(
						JSON.stringify({
							text: '{"model":"claude-opus-5","messages":[{"role": "user", "content": "REBUILT_BODY_MARKER"}],"stream":true}',
							byte_size: 41_118,
							has_body: true,
							reconstructed: true
						}),
						{ status: 200, headers: { 'content-type': 'application/json' } }
					);
				}
				if (url.includes('/manifest')) {
					return new Response(JSON.stringify({ request_id: 206, manifest: null }), {
						status: 200,
						headers: { 'content-type': 'application/json' }
					});
				}
				return origFetch(input);
			}) as typeof fetch;
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const { within, expect, waitFor, userEvent } = await import('storybook/test');
		const canvas = within(canvasElement);
		const panel = canvasElement.querySelector(
			'[data-slot="request-body-tab-original"]'
		) as HTMLElement;
		await expect(panel).toBeTruthy();
		await expect(canvas.queryByText(/Rebuilt from the stored envelope/i)).toBeNull();
		await userEvent.click(canvas.getByRole('button', { name: 'View full request body' }));
		await waitFor(() =>
			expect(canvas.getByText(/Rebuilt from the stored envelope/i)).toBeInTheDocument()
		);
	}}
>
	{#snippet template(args)}
		<RequestBodyCard {...args} />
	{/snippet}
</Story>
