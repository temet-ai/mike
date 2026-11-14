<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import { messages } from '../__fixtures__/wright';
	import WrightConversation from './WrightConversation.svelte';

	const { Story } = defineMeta({
		title: 'Wright/WrightConversation',
		component: WrightConversation,
		tags: ['autodocs'],
		parameters: { layout: 'padded' }
	});

	const onSend = fn(async () => ({ ok: true, id: 5 }));
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const box = canvas.getByRole('textbox', { name: 'Message Wright' });
		const send = canvas.getByRole('button', { name: 'Send' });
		await expect(send).toBeDisabled();
		await userEvent.type(box, 'What did the launch fixer do?');
		await expect(send).toBeEnabled();
		await userEvent.click(send);
		await expect(onSend).toHaveBeenCalledWith('What did the launch fixer do?');
		await expect(canvas.getByText('Wright is reading the evidence')).toBeVisible();
		const list = canvas.getByRole('list', { name: 'Conversation with Wright' });
		await expect(list.scrollTop).toBeGreaterThanOrEqual(0);
	}}
>
	{#snippet template()}
		<div style="max-inline-size: 44rem; container-type: inline-size;">
			<WrightConversation
				messages={messages()}
				status="ready"
				error={null}
				pendingReply={true}
				paired={true}
				sending={false}
				sendError={null}
				undo={{}}
				{onSend}
				onUndoSteer={() => {}}
				onRetry={() => {}}
			/>
		</div>
	{/snippet}
</Story>
