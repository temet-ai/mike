<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import * as Dialog from './index';
	import { Button } from '../button';

	const { Story } = defineMeta({
		title: 'UI/Dialog',
		component: Dialog.Root,
		tags: ['autodocs']
	});

	let controlledOpen = $state(true);
</script>

<Story name="Default">
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Confirm action</Dialog.Title>
					<Dialog.Description>
						This is a default dialog showing the title, description, and the mandatory close button
						rendered by the primitive.
					</Dialog.Description>
				</Dialog.Header>
				<Dialog.Footer>
					<Button variant="outline">Cancel</Button>
					<Button variant="default">Confirm</Button>
				</Dialog.Footer>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="WithDescription">
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Save request bundle</Dialog.Title>
					<Dialog.Description>
						Exercises the title + description pair with a longer body and two primary actions, so
						axe-core can verify ARIA wiring on a realistic dialog shape.
					</Dialog.Description>
				</Dialog.Header>
				<Dialog.Footer>
					<Button variant="outline">Cancel</Button>
					<Button variant="default">Save</Button>
				</Dialog.Footer>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="ControlledOpen">
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Button variant="outline" onclick={() => (controlledOpen = !controlledOpen)}>
				Toggle dialog (bound state: {controlledOpen ? 'open' : 'closed'})
			</Button>
			<Dialog.Root bind:open={controlledOpen}>
				<Dialog.Content class="sm:max-w-lg">
					<Dialog.Header>
						<Dialog.Title>Controlled dialog</Dialog.Title>
						<Dialog.Description>
							This dialog's open state is two-way bound to a parent-owned reactive boolean via
							bind:open — the production usage shape. Closing it writes false back through the
							binding.
						</Dialog.Description>
					</Dialog.Header>
					<Dialog.Footer>
						<Dialog.Close>
							{#snippet child({ props })}
								<Button variant="outline" {...props}>Close</Button>
							{/snippet}
						</Dialog.Close>
					</Dialog.Footer>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>
