<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import RuntimeSessionPanel from './RuntimeSessionPanel.svelte';
	import {
		buildRuntimeItem,
		buildRuntimeItemPage,
		buildRuntimeObservation
	} from '$lib/test-support/runtimeFixtures';

	const SESSION = 'f5f2e486-3f2f-413f-8c4e-419227f994d3';
	const requestedPages: number[] = [];
	const { Story } = defineMeta({
		title: 'Shell/RuntimeSessionPanel',
		component: RuntimeSessionPanel,
		tags: ['autodocs']
	});

	function installFetch(): void {
		requestedPages.length = 0;
		globalThis.fetch = (async (input: RequestInfo | URL) => {
			const url = new URL(
				input instanceof Request ? input.url : input.toString(),
				'http://localhost'
			);
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
		initialItems: buildRuntimeItemPage()
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
	}}
>
	{#snippet template(args)}
		<div class="bg-[var(--surface-bg)] p-4"><RuntimeSessionPanel {...args} /></div>
	{/snippet}
</Story>
