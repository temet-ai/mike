<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import RuntimeSessionPanel from './RuntimeSessionPanel.svelte';
	import {
		buildRuntimeCaptureExchange,
		buildRuntimeItem,
		buildRuntimeItemPage,
		buildRuntimeObservation
	} from '$lib/test-support/runtimeFixtures';

	const SESSION = 'f5f2e486-3f2f-413f-8c4e-419227f994d3';
	const requestedPages: number[] = [];
	const bodyReads: Array<{ direction: string; cache?: RequestCache }> = [];
	const exchange = buildRuntimeCaptureExchange({ status: 'aborted' });
	const requestBody = JSON.stringify(
		{
			input: '<img src="invalid" onerror="alert(1)"> remains literal captured text.',
			unknown_provider_field: { full_value: 'Every unknown field remains available.' },
			ending: 'Final captured byte.'
		},
		null,
		2
	);
	const responseBody =
		'event: response.output_text.delta\ndata: {"delta":"Reply before interruption","unknown_field":true}\n\n';
	let pendingBodySignal: AbortSignal | null | undefined;
	let releaseBody: (() => void) | null = null;

	function jsonResponse(body: unknown): Response {
		return new Response(JSON.stringify(body), { headers: { 'content-type': 'application/json' } });
	}

	function capturedBody(body: BodyInit, complete = true): Response {
		return new Response(body, { headers: { 'x-mike-capture-complete': String(complete) } });
	}
	const { Story } = defineMeta({
		title: 'Shell/RuntimeSessionPanel',
		component: RuntimeSessionPanel,
		tags: ['autodocs']
	});

	function installFetch(): void {
		requestedPages.length = 0;
		bodyReads.length = 0;
		pendingBodySignal = null;
		releaseBody = null;
		let listReads = 0;
		let requestReads = 0;
		globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
			const url = new URL(
				input instanceof Request ? input.url : input.toString(),
				'http://localhost'
			);
			if (url.pathname.includes('/body/')) {
				const direction = url.pathname.split('/').at(-1)!;
				bodyReads.push({ direction, cache: init?.cache });
				if (direction === 'response') return capturedBody(responseBody, false);
				if (direction === 'sent')
					return capturedBody(new Uint8Array([0x28, 0xb5, 0x2f, 0xfd, 0, 255]));
				requestReads += 1;
				if (requestReads === 1) return new Response(null, { status: 404, statusText: 'Not Found' });
				if (requestReads === 3) {
					pendingBodySignal = init?.signal;
					return new Promise<Response>((resolve) => {
						releaseBody = () => resolve(capturedBody(requestBody));
					});
				}
				return capturedBody(requestBody);
			}
			if (url.pathname.endsWith('/runtime-captures')) {
				listReads += 1;
				if (listReads === 1) return new Response(null, { status: 404, statusText: 'Not Found' });
				return jsonResponse({ exchanges: [exchange], next_offset: 20 });
			}
			const offset = Number(url.searchParams.get('offset'));
			requestedPages.push(offset);
			return new Response(
				JSON.stringify({
					observation: buildRuntimeObservation(),
					runtime_items: buildRuntimeItemPage(
						offset === 0
							? {}
							: {
									items: [
										buildRuntimeItem({
											item_id: 'tool-2',
											name: 'run full validation',
											success: false,
											exit_code: 1,
											status: 'failed'
										})
									],
									offset: 1,
									has_more: false
								}
					)
				}),
				{ status: 200, headers: { 'content-type': 'application/json' } }
			);
		}) as typeof fetch;
	}
</script>

<Story
	name="Default"
	args={{
		sessionId: SESSION,
		observation: buildRuntimeObservation(),
		initialItems: buildRuntimeItemPage({ items: [], has_more: false })
	}}
	loaders={[
		async () => {
			installFetch();
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const facts = canvas.getByLabelText('Runtime observation facts');
		await expect(within(facts).getByText('fixture_runtime')).toBeVisible();
		await expect(within(facts).getByText('fixture_adapter')).toBeVisible();
		await expect(within(facts).getByText('requested-model')).toBeVisible();
		for (const label of ['Observed model', 'Input tokens', 'Total tokens', 'Reported cost']) {
			await expect(
				within(facts).getByText(label).parentElement?.querySelector('dd')
			).toHaveTextContent('Unknown');
		}
		await expect(
			within(facts).getByText('Output tokens').parentElement?.querySelector('dd')
		).toHaveTextContent('0');
		await expect(canvas.getByText('No runtime activity items on this page.')).toBeVisible();
		await waitFor(() =>
			expect(canvas.getByRole('button', { name: 'Retry captures' })).toBeVisible()
		);
		await userEvent.click(canvas.getByRole('button', { name: 'Retry captures' }));
		await waitFor(() => expect(canvas.getByText('POST /v1/responses')).toBeVisible());
		await expect(bodyReads).toHaveLength(0);
		await userEvent.click(canvas.getByRole('button', { name: 'View exchange details' }));
		await expect(
			canvas.getByRole('region', { name: 'Exchange details' }).querySelector('pre')?.textContent
		).toBe(JSON.stringify(exchange, null, 2));
		await userEvent.click(canvas.getByRole('button', { name: 'Hide exchange details' }));
		await userEvent.click(canvas.getByRole('button', { name: 'View request' }));
		await waitFor(() =>
			expect(
				within(canvas.getByRole('region', { name: 'Captured body' })).getByRole('alert')
			).toHaveTextContent('404 Not Found')
		);
		await userEvent.click(canvas.getByRole('button', { name: 'Retry body' }));
		await waitFor(() =>
			expect(
				canvas.getByRole('region', { name: 'Captured body' }).querySelector('pre')?.textContent
			).toBe(requestBody)
		);
		await expect(
			canvas.getByRole('region', { name: 'Captured body' }).querySelector('img')
		).toBeNull();
		await expect(bodyReads).toEqual([
			{ direction: 'request', cache: 'no-store' },
			{ direction: 'request', cache: 'no-store' }
		]);
		await userEvent.click(canvas.getByRole('button', { name: 'View response' }));
		await waitFor(() =>
			expect(
				canvas.getByRole('region', { name: 'Captured body' }).querySelector('pre')?.textContent
			).toBe(responseBody)
		);
		await expect(
			canvas.getByText('Capture was interrupted. These are the bytes received before it stopped.')
		).toBeVisible();
		await expect(
			canvasElement.querySelectorAll('[data-slot="message-view-code-block"]')
		).toHaveLength(1);
		await userEvent.click(canvas.getByRole('button', { name: 'View sent request' }));
		await waitFor(() =>
			expect(
				canvas.getByText('This body is binary. Download it to read the complete captured bytes.')
			).toBeVisible()
		);
		await expect(
			canvas.getByRole('region', { name: 'Captured body' }).querySelector('pre')
		).toBeNull();
		await expect(canvas.getByRole('link', { name: 'Download sent request' })).toHaveAttribute(
			'href',
			`/api/sessions/${SESSION}/runtime-captures/${exchange.id}/body/sent`
		);
		await expect(canvas.getByRole('link', { name: 'Download sent request' })).toHaveAttribute(
			'download',
			`${exchange.id}-sent.bin`
		);
		await userEvent.click(canvas.getByRole('button', { name: 'View request' }));
		await waitFor(() => expect(releaseBody).not.toBeNull());
		await userEvent.click(canvas.getByRole('button', { name: 'Hide request' }));
		await expect(pendingBodySignal?.aborted).toBe(true);
		releaseBody?.();
		await expect(canvas.queryByRole('region', { name: 'Captured body' })).not.toBeInTheDocument();
		await userEvent.click(canvas.getByRole('button', { name: 'Refresh runtime activity' }));
		await waitFor(() => expect(canvas.getByText('command · cargo check')).toBeVisible());
		await expect(canvas.getByText('command · cargo check')).toBeVisible();
		await expect(canvas.getByText('not_collected')).toBeVisible();
		await userEvent.click(canvas.getByRole('button', { name: 'Next activity page' }));
		await waitFor(() => expect(canvas.getByText('command · run full validation')).toBeVisible());
		await expect(requestedPages).toContain(1);
		await expect(canvas.queryByText('command · cargo check')).not.toBeInTheDocument();
		await expect(canvas.getByRole('button', { name: 'Next activity page' })).toBeDisabled();
		await userEvent.click(canvas.getByRole('button', { name: 'Previous activity page' }));
		await waitFor(() => expect(canvas.getByText('command · cargo check')).toBeVisible());
		await expect(requestedPages).toContain(0);
		const callsBeforeRefresh = requestedPages.length;
		await userEvent.click(canvas.getByRole('button', { name: 'Refresh runtime activity' }));
		await waitFor(() => expect(requestedPages.length).toBeGreaterThan(callsBeforeRefresh));
		await userEvent.click(canvas.getByRole('button', { name: 'View request' }));
		await waitFor(() =>
			expect(
				canvas.getByRole('region', { name: 'Captured body' }).querySelector('pre')?.textContent
			).toBe(requestBody)
		);
	}}
>
	{#snippet template(args)}
		<div class="bg-[var(--surface-bg)] p-4"><RuntimeSessionPanel {...args} /></div>
	{/snippet}
</Story>
