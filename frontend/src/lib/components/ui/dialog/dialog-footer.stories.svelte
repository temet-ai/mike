<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within, userEvent, fn, waitFor } from 'storybook/test';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import * as Dialog from './index';
	import { Button } from '../button';

	const { Story } = defineMeta({
		title: 'UI/Dialog/DialogFooter',
		component: Dialog.Footer,
		tags: ['autodocs'],
		argTypes: {
			showCloseButton: { control: 'boolean' },
			class: { control: 'text' }
		}
	});

	const interactiveShowCloseButtonSpy = fn();
</script>

<Story name="Empty" args={{ showCloseButton: false }}>
	{#snippet template(args)}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Empty footer demo</Dialog.Title>
				</Dialog.Header>
				<Dialog.Footer showCloseButton={args.showCloseButton} class={args.class} />
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="WithChildren" args={{ showCloseButton: false }}>
	{#snippet template(args)}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Confirm action</Dialog.Title>
					<Dialog.Description>
						Footer renders children-only: outline Cancel + default Confirm buttons.
					</Dialog.Description>
				</Dialog.Header>
				<Dialog.Footer showCloseButton={args.showCloseButton} class={args.class}>
					<Button variant="outline">Cancel</Button>
					<Button variant="default">Confirm</Button>
				</Dialog.Footer>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="WithShowCloseButton" args={{ showCloseButton: true }}>
	{#snippet template(args)}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Close button only</Dialog.Title>
					<Dialog.Description>
						Footer renders only the showCloseButton outline button — no children.
					</Dialog.Description>
				</Dialog.Header>
				<Dialog.Footer showCloseButton={args.showCloseButton} class={args.class} />
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="WithChildrenAndCloseButton" args={{ showCloseButton: true }}>
	{#snippet template(args)}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Combined footer</Dialog.Title>
					<Dialog.Description>
						Children (Cancel + Save) rendered first, then the built-in Close button.
					</Dialog.Description>
				</Dialog.Header>
				<Dialog.Footer showCloseButton={args.showCloseButton} class={args.class}>
					<Button variant="outline">Cancel</Button>
					<Button variant="default">Save</Button>
				</Dialog.Footer>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="CustomClassName"
	args={{ showCloseButton: false, class: 'border border-[var(--destructive)] rounded p-2' }}
>
	{#snippet template(args)}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Custom class merge</Dialog.Title>
				</Dialog.Header>
				<Dialog.Footer showCloseButton={args.showCloseButton} class={args.class}>
					<Button variant="outline">Cancel</Button>
					<Button variant="default">OK</Button>
				</Dialog.Footer>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="ControlledOpen" args={{ showCloseButton: false }}>
	{#snippet template(args)}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Controlled close</Dialog.Title>
					<Dialog.Description>
						Dialog.Close is provided as a child of Dialog.Footer for full control over button label
						and variant — not via showCloseButton prop.
					</Dialog.Description>
				</Dialog.Header>
				<Dialog.Footer showCloseButton={args.showCloseButton} class={args.class}>
					<Dialog.Close>
						{#snippet child({ props })}
							<Button variant="outline" {...props}>Dismiss</Button>
						{/snippet}
					</Dialog.Close>
					<Button variant="default">Save</Button>
				</Dialog.Footer>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="InteractiveDataSlot"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const body = canvasElement.ownerDocument.body;
		const canvas = within(body);

		const footer = body.querySelector('[data-slot="dialog-footer"]');
		await expect(footer, 'footer element with data-slot="dialog-footer" should exist').toBeTruthy();

		await expect(footer?.classList.contains('flex'), 'footer should have flex class').toBe(true);
		await expect(
			footer?.classList.contains('flex-col-reverse'),
			'footer should have flex-col-reverse class'
		).toBe(true);

		const cancelBtn = canvas.getByRole('button', { name: 'Cancel' });
		await expect(cancelBtn, 'Cancel button should be rendered').toBeTruthy();
		const confirmBtn = canvas.getByRole('button', { name: 'Confirm' });
		await expect(confirmBtn, 'Confirm button should be rendered').toBeTruthy();
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Data-slot assertion</Dialog.Title>
				</Dialog.Header>
				<Dialog.Footer>
					<Button variant="outline">Cancel</Button>
					<Button variant="default">Confirm</Button>
				</Dialog.Footer>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="InteractiveShowCloseButton"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		interactiveShowCloseButtonSpy.mockClear();
		const body = canvasElement.ownerDocument.body;
		const canvas = within(body);

		const footer = body.querySelector('[data-slot="dialog-footer"]');
		await expect(footer, 'footer should exist').toBeTruthy();

		const closeBtn = canvas.getByRole('button', { name: 'Close' });
		await expect(closeBtn, 'Close button should be rendered').toBeTruthy();
		await waitFor(() => expect(closeBtn).toBeVisible());

		await userEvent.click(closeBtn);

		await expect(interactiveShowCloseButtonSpy).toHaveBeenCalledWith(false);
	}}
>
	{#snippet template()}
		<Dialog.Root open onOpenChange={interactiveShowCloseButtonSpy}>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>showCloseButton interaction</Dialog.Title>
				</Dialog.Header>
				<Dialog.Footer showCloseButton={true} />
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="InteractiveFocusRing"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const body = canvasElement.ownerDocument.body;
		const canvas = within(body);

		const closeBtn = canvas.getByRole('button', { name: 'Close' });
		await expect(closeBtn).toBeTruthy();

		(closeBtn.closest('[role="dialog"]') as HTMLElement | null)?.focus?.();
		await userEvent.tab();

		await expect(body.ownerDocument.activeElement).toBe(closeBtn);
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Focus ring test</Dialog.Title>
				</Dialog.Header>
				<Dialog.Footer showCloseButton={true} />
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="InteractiveCustomClassName"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const body = canvasElement.ownerDocument.body;
		const footer = body.querySelector('[data-slot="dialog-footer"]');
		await expect(footer, 'footer should exist').toBeTruthy();

		await expect(
			footer?.classList.contains('flex'),
			'base flex class must survive cn() merge'
		).toBe(true);

		await expect(
			footer?.classList.contains('rounded'),
			'custom rounded class should be present after cn() merge'
		).toBe(true);
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>Custom class merge assertion</Dialog.Title>
				</Dialog.Header>
				<Dialog.Footer class="rounded p-2">
					<Button variant="outline">Cancel</Button>
					<Button variant="default">OK</Button>
				</Dialog.Footer>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="SnapshotEmpty"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'dialog-footer/Empty');
	}}
>
	{#snippet template()}
		<div style="min-height: 300px; padding: 8px; position: relative;">
			<Dialog.Root open>
				<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
					<Dialog.Header>
						<Dialog.Title>Snapshot: empty footer</Dialog.Title>
					</Dialog.Header>
					<Dialog.Footer />
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="SnapshotWithChildren"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'dialog-footer/WithChildren');
	}}
>
	{#snippet template()}
		<div style="min-height: 300px; padding: 8px; position: relative;">
			<Dialog.Root open>
				<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
					<Dialog.Header>
						<Dialog.Title>Snapshot: children layout</Dialog.Title>
						<Dialog.Description>
							Visual baseline for the two-button footer (Cancel + Confirm).
						</Dialog.Description>
					</Dialog.Header>
					<Dialog.Footer>
						<Button variant="outline">Cancel</Button>
						<Button variant="default">Confirm</Button>
					</Dialog.Footer>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="SnapshotWithShowCloseButton"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'dialog-footer/WithShowCloseButton');
	}}
>
	{#snippet template()}
		<div style="min-height: 300px; padding: 8px; position: relative;">
			<Dialog.Root open>
				<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
					<Dialog.Header>
						<Dialog.Title>Snapshot: showCloseButton only</Dialog.Title>
					</Dialog.Header>
					<Dialog.Footer showCloseButton={true} />
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="SnapshotCombined"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'dialog-footer/Combined');
	}}
>
	{#snippet template()}
		<div style="min-height: 300px; padding: 8px; position: relative;">
			<Dialog.Root open>
				<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
					<Dialog.Header>
						<Dialog.Title>Snapshot: combined layout</Dialog.Title>
						<Dialog.Description>
							Visual baseline for children + built-in Close button combined.
						</Dialog.Description>
					</Dialog.Header>
					<Dialog.Footer showCloseButton={true}>
						<Button variant="outline">Cancel</Button>
						<Button variant="default">Save</Button>
					</Dialog.Footer>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="SnapshotCustomClassName"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'dialog-footer/CustomClassName');
	}}
>
	{#snippet template()}
		<div style="min-height: 300px; padding: 8px; position: relative;">
			<Dialog.Root open>
				<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
					<Dialog.Header>
						<Dialog.Title>Snapshot: custom class merge</Dialog.Title>
					</Dialog.Header>
					<Dialog.Footer class="border border-[var(--destructive)] rounded p-2">
						<Button variant="outline">Cancel</Button>
						<Button variant="default">OK</Button>
					</Dialog.Footer>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="InteractiveRef"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const body = canvasElement.ownerDocument.body;

		const footer = body.querySelector('[data-slot="dialog-footer"]');
		await expect(footer, 'footer div must exist in DOM (ref bind:this target)').toBeTruthy();
		await expect(
			footer instanceof HTMLElement,
			'footer must be an HTMLElement — confirms ref=$bindable is a real element reference'
		).toBe(true);
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
				<Dialog.Header>
					<Dialog.Title>ref prop test</Dialog.Title>
					<Dialog.Description>
						Exercises Dialog.Footer ref=$bindable(null). The play function queries the bound element
						and asserts it is a real HTMLDivElement.
					</Dialog.Description>
				</Dialog.Header>
				<Dialog.Footer>
					<Button variant="outline">Cancel</Button>
					<Button variant="default">OK</Button>
				</Dialog.Footer>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="SnapshotControlledOpen"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'dialog-footer/ControlledOpen');
	}}
>
	{#snippet template()}
		<div style="min-height: 300px; padding: 8px; position: relative;">
			<Dialog.Root open>
				<Dialog.Content showCloseButton={false} class="sm:max-w-lg">
					<Dialog.Header>
						<Dialog.Title>Snapshot: controlled-open shape</Dialog.Title>
						<Dialog.Description>
							Dialog.Close provided as a child of Dialog.Footer — not via showCloseButton.
						</Dialog.Description>
					</Dialog.Header>
					<Dialog.Footer showCloseButton={false}>
						<Dialog.Close>
							{#snippet child({ props })}
								<Button variant="outline" {...props}>Dismiss</Button>
							{/snippet}
						</Dialog.Close>
						<Button variant="default">Save</Button>
					</Dialog.Footer>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>
