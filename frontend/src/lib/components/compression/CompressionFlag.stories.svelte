<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within, waitFor } from 'storybook/test';
	import CompressionFlag from './CompressionFlag.svelte';
	let captured: Record<string, unknown> | undefined;
	const { Story } = defineMeta({
		title: 'Components/Compression/Flag',
		component: CompressionFlag,
		beforeEach: () => {
			captured = undefined;
			const previous = globalThis.fetch;
			globalThis.fetch = (async (_input, init) => {
				captured = JSON.parse(String(init?.body));
				return new Response(JSON.stringify({ id: 'flag', added: true }), { status: 200 });
			}) as typeof fetch;
			return () => {
				globalThis.fetch = previous;
			};
		}
	});
</script>

<Story
	name="FlagToolResult"
	args={{ target: { kind: 'tool_result', request_id: 473363, tool_use_id: 808536 } }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await userEvent.click(canvas.getByRole('button', { name: 'Flag for compression' }));
		const dialog = within(document.body);
		await userEvent.type(dialog.getByLabelText('Reason'), 'Preserve exact failures');
		await userEvent.type(dialog.getByLabelText('Evidence'), 'Follow-up 473366');
		await userEvent.click(dialog.getByRole('button', { name: 'Save flag' }));
		await waitFor(() =>
			expect(canvas.getByRole('button', { name: 'Flagged for review' })).toBeVisible()
		);
		await waitFor(() => expect(document.body.style.pointerEvents).not.toBe('none'));
		expect(captured).toMatchObject({
			kind: 'tool_result',
			request_id: 473363,
			tool_use_id: 808536,
			reason: 'Preserve exact failures'
		});
	}}
/>
<Story
	name="UnavailableServer"
	args={{ target: { kind: 'response', request_id: 42 } }}
	beforeEach={() => {
		globalThis.fetch = (async () => new Response('', { status: 503 })) as typeof fetch;
	}}
	play={async ({ canvasElement }) => {
		await userEvent.click(
			within(canvasElement).getByRole('button', { name: 'Flag for compression' })
		);
		const body = within(document.body);
		await userEvent.type(body.getByLabelText('Reason'), 'Repeated status');
		await userEvent.type(body.getByLabelText('Evidence'), 'Response 42');
		await userEvent.click(body.getByRole('button', { name: 'Save flag' }));
		await waitFor(() => expect(body.getByText('Flag was not saved')).toBeVisible());
		expect(body.getByLabelText('Reason')).toHaveValue('Repeated status');
		await userEvent.keyboard('{Escape}');
		await waitFor(() => expect(document.body.style.pointerEvents).not.toBe('none'));
	}}
/>
