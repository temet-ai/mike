<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { fn, userEvent, waitFor, within, expect } from 'storybook/test';
	import * as Tooltip from './index';
	import { Button } from '../button';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';

	let refBindingTarget: HTMLButtonElement | null = null;

	const { Story } = defineMeta({
		title: 'UI/Tooltip/TooltipTrigger',
		component: Tooltip.Trigger,
		tags: ['autodocs'],
		args: {
			onclick: fn()
		},
		argTypes: {
			disabled: {
				control: 'boolean',
				description: 'When true, the trigger is disabled and the tooltip will not open'
			},
			class: {
				control: 'text',
				description: 'Extra CSS classes forwarded to the bits-ui trigger element'
			},
			onclick: {
				description: 'Click handler forwarded through the child snippet props to the inner element'
			}
		}
	});
</script>

<Story name="Default">
	{#snippet template()}
		<div class="flex items-center justify-center p-20">
			<Tooltip.Root open>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Hover for tooltip</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content>Tooltip content</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story name="Disabled">
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Tooltip.Root>
				<Tooltip.Trigger disabled>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" disabled {...props}>Disabled trigger</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content>This should not appear</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story name="IconOnly">
	{#snippet template()}
		<div class="flex items-center justify-center gap-4 p-16">
			<Tooltip.Root open>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="ghost" size="icon" aria-label="Copy request body" {...props}>
							<svg
								class="size-4"
								xmlns="http://www.w3.org/2000/svg"
								fill="none"
								viewBox="0 0 24 24"
								stroke="currentColor"
								aria-hidden="true"
							>
								<path
									stroke-linecap="round"
									stroke-linejoin="round"
									stroke-width="2"
									d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z"
								/>
							</svg>
						</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content>Copy request body</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story name="AsAnchor">
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Tooltip.Root open>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<a href="#anchor-example" class="text-sm underline underline-offset-2" {...props}>
							Anchor trigger
						</a>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content>Tooltip anchored to a link element</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="CustomClass"
	play={async ({ canvasElement }) => {
		const trigger = canvasElement.querySelector('[data-slot="tooltip-trigger"]');

		await expect(
			trigger?.classList.contains('rounded'),
			'trigger should have the "rounded" class from the class prop'
		).toBe(true);
		await expect(
			trigger?.classList.contains('ring-2'),
			'trigger should have the "ring-2" class from the class prop'
		).toBe(true);
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Tooltip.Root open>
				<Tooltip.Trigger class="rounded ring-2 ring-[var(--interactive-active-fg)]">
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Custom class</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content>Class forwarded to trigger wrapper</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="RefBinding"
	tags={['interactive']}
	args={{ onclick: fn() }}
	play={async ({ args, canvasElement }) => {
		const canvas = within(canvasElement);

		const button = canvas.getByRole('button', { name: 'Ref binding target' });

		await userEvent.click(button);
		await expect(
			args.onclick,
			'onclick spy should be called after clicking the trigger button'
		).toHaveBeenCalledWith(expect.any(MouseEvent));

		await expect(
			refBindingTarget instanceof HTMLButtonElement,
			'ref bindable should resolve to the inner HTMLButtonElement, not null or a wrapper'
		).toBe(true);

		await expect(
			refBindingTarget,
			'ref should point to the same node returned by getByRole("button")'
		).toBe(button);
	}}
>
	{#snippet template(args)}
		<div class="flex items-center justify-center p-20">
			<Tooltip.Root open>
				<Tooltip.Trigger bind:ref={refBindingTarget} onclick={args.onclick}>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Ref binding target</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content>Ref binding story</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="TriggerRendered"
	tags={['interactive']}
	args={{ onclick: fn() }}
	play={async ({ args, canvasElement }) => {
		const canvas = within(canvasElement);

		await waitFor(() => {
			expect(
				canvasElement.querySelector('[data-slot="tooltip-trigger"]'),
				'trigger element with data-slot="tooltip-trigger" should be in the DOM'
			).toBeTruthy();
		});
		const trigger = canvasElement.querySelector('[data-slot="tooltip-trigger"]')!;

		await expect(
			trigger.hasAttribute('data-disabled'),
			'trigger should not have data-disabled when enabled'
		).toBe(false);

		const button = canvas.getByRole('button');
		await expect(button, 'inner button should be in the DOM').toBeTruthy();

		await waitFor(() => {
			expect(
				document.body.querySelector('[data-testid="trigger-rendered-tooltip-content"]'),
				'portalled tooltip-content should be in document.body when tooltip is open'
			).not.toBeNull();
		});
		const tooltipContent = document.body.querySelector(
			'[data-testid="trigger-rendered-tooltip-content"]'
		)!;
		await expect(
			tooltipContent.textContent?.trim(),
			'portalled tooltip content should contain the expected text'
		).toBe('DOM assertion target');

		await userEvent.click(button);
		await expect(
			args.onclick,
			'onclick spy should be called after clicking the trigger button'
		).toHaveBeenCalledWith(expect.any(MouseEvent));
	}}
>
	{#snippet template(args)}
		<div class="flex items-center justify-center p-20">
			<Tooltip.Root open delayDuration={0}>
				<Tooltip.Trigger onclick={args.onclick}>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Hover me</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content data-testid="trigger-rendered-tooltip-content"
					>DOM assertion target</Tooltip.Content
				>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="ClickSpy"
	tags={['interactive']}
	args={{ onclick: fn() }}
	play={async ({ args, canvasElement }) => {
		const canvas = within(canvasElement);

		const button = canvas.getByRole('button', { name: 'Click spy target' });
		await expect(button, 'inner button should be in the DOM').toBeTruthy();

		await userEvent.click(button);

		await expect(
			args.onclick,
			'onclick spy should be called once with a MouseEvent via the bits-ui forwarding chain'
		).toHaveBeenCalledWith(expect.any(MouseEvent));
		await expect(args.onclick).toHaveBeenCalledOnce();
	}}
>
	{#snippet template(args)}
		<div class="flex items-center justify-center p-16">
			<Tooltip.Root>
				<Tooltip.Trigger onclick={args.onclick}>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Click spy target</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content>Click to fire onclick spy</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="DisabledState"
	tags={['interactive']}
	args={{ onclick: fn() }}
	play={async ({ args, canvasElement }) => {
		const canvas = within(canvasElement);

		const trigger = canvasElement.querySelector('[data-slot="tooltip-trigger"]');
		await expect(
			trigger,
			'trigger element with data-slot="tooltip-trigger" should be in the DOM'
		).toBeTruthy();
		await expect(
			trigger?.hasAttribute('data-disabled'),
			'trigger should have data-disabled attribute when disabled prop is set'
		).toBe(true);

		const button = canvas.getByRole('button', { name: 'Disabled' });
		await userEvent.click(button, { pointerEventsCheck: 0 });
		await expect(
			args.onclick,
			'onclick spy should NOT be called when the trigger is disabled'
		).not.toHaveBeenCalled();
	}}
>
	{#snippet template(args)}
		<div class="flex items-center justify-center p-16">
			<Tooltip.Root>
				<Tooltip.Trigger disabled onclick={args.onclick}>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" disabled {...props}>Disabled</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content>Should not open</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="TriggerSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'tooltip-trigger/Default');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-20">
			<Tooltip.Root open>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Snapshot trigger</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content>Visual baseline</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="AllVariantsSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'tooltip-trigger/AllVariants');
	}}
>
	{#snippet template()}
		<div class="flex flex-wrap items-center justify-center gap-8 p-20">
			<Tooltip.Root open>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Text trigger</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content>Text trigger baseline</Tooltip.Content>
			</Tooltip.Root>

			<Tooltip.Root open>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="ghost" size="icon" aria-label="Settings" {...props}>
							<svg
								class="size-4"
								xmlns="http://www.w3.org/2000/svg"
								fill="none"
								viewBox="0 0 24 24"
								stroke="currentColor"
								aria-hidden="true"
							>
								<path
									stroke-linecap="round"
									stroke-linejoin="round"
									stroke-width="2"
									d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"
								/>
								<path
									stroke-linecap="round"
									stroke-linejoin="round"
									stroke-width="2"
									d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"
								/>
							</svg>
						</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content>Settings</Tooltip.Content>
			</Tooltip.Root>

			<Tooltip.Root>
				<Tooltip.Trigger disabled>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" disabled {...props}>Disabled</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content>Should not open</Tooltip.Content>
			</Tooltip.Root>

			<Tooltip.Root open>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<a href="#anchor" class="text-sm underline underline-offset-2" {...props}>
							Anchor trigger
						</a>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content>Anchor as trigger</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>
