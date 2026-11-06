<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { fn, userEvent, within, expect, waitFor } from 'storybook/test';
	import { Tooltip as TooltipPrimitive } from 'bits-ui';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import TooltipProvider from './tooltip-provider.svelte';
	import * as Tooltip from './index';
	import { Button } from '../button';

	const { Story } = defineMeta({
		title: 'UI/Tooltip/TooltipProvider',
		component: TooltipProvider,
		tags: ['autodocs'],
		argTypes: {
			delayDuration: {
				control: { type: 'number', min: 0, max: 2000, step: 50 },
				description: 'Milliseconds before tooltip opens after hover/focus. Default: 120.'
			},
			skipDelayDuration: {
				control: { type: 'number', min: 0, max: 1000, step: 50 },
				description:
					'Window (ms) after a tooltip closes during which re-entering another trigger skips the delay. Default: 300.'
			},
			disableHoverableContent: {
				control: 'boolean',
				description:
					'When true, the tooltip closes as soon as the pointer leaves the trigger (content is not hoverable). Default: true.'
			},
			disableCloseOnTriggerClick: {
				control: 'boolean',
				description: 'When true, clicking the trigger does not close the tooltip. Default: false.'
			},
			disabled: {
				control: 'boolean',
				description:
					'When true, all descendant tooltips are prevented from opening. Default: false.'
			},
			ignoreNonKeyboardFocus: {
				control: 'boolean',
				description:
					'When true, tooltips only open when focus arrives via keyboard (Tab/Shift+Tab), not mouse click. Default: false.'
			},
			...({
				onOpenChange: {
					description:
						'Spy prop — passed to the child TooltipPrimitive.Root in interactive stories as an fn() spy. Assertions use toHaveBeenCalledWith directly; this is NOT wired via the Storybook Actions panel.',
					table: { category: 'Interaction Spy' }
				}
			} as Record<string, unknown>)
		}
	} as Parameters<typeof defineMeta>[0]);
</script>

<Story name="Default" args={{ delayDuration: 120, disableHoverableContent: true }}>
	{#snippet template(args)}
		<div class="flex items-center justify-center gap-6 p-16">
			<TooltipProvider {...args}>
				<Tooltip.Root open>
					<Tooltip.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" size="sm" {...props}>Copy</Button>
						{/snippet}
					</Tooltip.Trigger>
					<Tooltip.Content>Copy the API token</Tooltip.Content>
				</Tooltip.Root>

				<Tooltip.Root open>
					<Tooltip.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" size="sm" {...props}>Archive</Button>
						{/snippet}
					</Tooltip.Trigger>
					<Tooltip.Content>Archive this session</Tooltip.Content>
				</Tooltip.Root>
			</TooltipProvider>
		</div>
	{/snippet}
</Story>

<Story name="Delayed" args={{ delayDuration: 800, disableHoverableContent: true }}>
	{#snippet template(args)}
		<div class="flex items-center justify-center gap-6 p-16">
			<TooltipProvider {...args}>
				<Tooltip.Root open>
					<Tooltip.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" size="sm" {...props}>Pin session</Button>
						{/snippet}
					</Tooltip.Trigger>
					<Tooltip.Content>Pin to keep beyond TTL (800ms delay)</Tooltip.Content>
				</Tooltip.Root>

				<Tooltip.Root>
					<Tooltip.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" size="sm" {...props}>Delete</Button>
						{/snippet}
					</Tooltip.Trigger>
					<Tooltip.Content>Permanently delete session</Tooltip.Content>
				</Tooltip.Root>
			</TooltipProvider>
		</div>
	{/snippet}
</Story>

<Story name="HoverableContent" args={{ delayDuration: 120, disableHoverableContent: false }}>
	{#snippet template(args)}
		<div class="flex items-center justify-center p-16">
			<TooltipProvider {...args}>
				<Tooltip.Root open>
					<Tooltip.Trigger>
						{#snippet child({ props })}
							<Button variant="ghost" size="sm" {...props}>View details</Button>
						{/snippet}
					</Tooltip.Trigger>
					<Tooltip.Content>
						Hover over this content — it stays open (disableHoverableContent=false)
					</Tooltip.Content>
				</Tooltip.Root>
			</TooltipProvider>
		</div>
	{/snippet}
</Story>

<Story
	name="SkipDelay"
	tags={['design-exploration']}
	args={{ delayDuration: 120, skipDelayDuration: 0, disableHoverableContent: true }}
>
	{#snippet template()}
		<div class="flex flex-col gap-8 p-8">
			<div class="flex items-center gap-4">
				<span class="w-48 shrink-0 text-xs text-[var(--text-2)]">skipDelay=300 (default)</span>
				<TooltipProvider delayDuration={120} skipDelayDuration={300} disableHoverableContent={true}>
					<Tooltip.Root open>
						<Tooltip.Trigger>
							{#snippet child({ props })}
								<Button variant="outline" size="sm" {...props}>First</Button>
							{/snippet}
						</Tooltip.Trigger>
						<Tooltip.Content
							>First tooltip — re-enter Second within 300ms for instant open</Tooltip.Content
						>
					</Tooltip.Root>

					<Tooltip.Root>
						<Tooltip.Trigger>
							{#snippet child({ props })}
								<Button variant="outline" size="sm" {...props}>Second</Button>
							{/snippet}
						</Tooltip.Trigger>
						<Tooltip.Content
							>Opens instantly if entered within 300ms of closing First</Tooltip.Content
						>
					</Tooltip.Root>
				</TooltipProvider>
			</div>

			<div class="flex items-center gap-4">
				<span class="w-48 shrink-0 text-xs text-[var(--text-2)]">skipDelay=0 (disabled)</span>
				<TooltipProvider delayDuration={120} skipDelayDuration={0} disableHoverableContent={true}>
					<Tooltip.Root open>
						<Tooltip.Trigger>
							{#snippet child({ props })}
								<Button variant="outline" size="sm" {...props}>First</Button>
							{/snippet}
						</Tooltip.Trigger>
						<Tooltip.Content>First tooltip (skipDelayDuration=0)</Tooltip.Content>
					</Tooltip.Root>

					<Tooltip.Root>
						<Tooltip.Trigger>
							{#snippet child({ props })}
								<Button variant="outline" size="sm" {...props}>Second</Button>
							{/snippet}
						</Tooltip.Trigger>
						<Tooltip.Content
							>Second — always incurs full delay (skipDelayDuration=0)</Tooltip.Content
						>
					</Tooltip.Root>
				</TooltipProvider>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="Disabled" args={{ disabled: true, delayDuration: 120, disableHoverableContent: true }}>
	{#snippet template(args)}
		<div class="flex items-center justify-center gap-6 p-16">
			<TooltipProvider {...args}>
				<Tooltip.Root>
					<Tooltip.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" size="sm" {...props}>Copy</Button>
						{/snippet}
					</Tooltip.Trigger>
					<Tooltip.Content>This should not appear</Tooltip.Content>
				</Tooltip.Root>

				<Tooltip.Root>
					<Tooltip.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" size="sm" {...props}>Archive</Button>
						{/snippet}
					</Tooltip.Trigger>
					<Tooltip.Content>This should not appear either</Tooltip.Content>
				</Tooltip.Root>
			</TooltipProvider>
		</div>
	{/snippet}
</Story>

<Story
	name="KeyboardFocusOnly"
	tags={['design-exploration']}
	args={{ ignoreNonKeyboardFocus: true, delayDuration: 120, disableHoverableContent: true }}
>
	{#snippet template(args)}
		<div class="flex items-center justify-center gap-6 p-16">
			<TooltipProvider {...args}>
				<Tooltip.Root open>
					<Tooltip.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" size="sm" {...props}>Tab to me</Button>
						{/snippet}
					</Tooltip.Trigger>
					<Tooltip.Content
						>Only opens on keyboard focus (ignoreNonKeyboardFocus=true)</Tooltip.Content
					>
				</Tooltip.Root>
			</TooltipProvider>
		</div>
	{/snippet}
</Story>

<Story
	name="NoCloseOnClick"
	tags={['design-exploration']}
	args={{ disableCloseOnTriggerClick: true, delayDuration: 120, disableHoverableContent: true }}
>
	{#snippet template(args)}
		<div class="flex items-center justify-center p-16">
			<TooltipProvider {...args}>
				<Tooltip.Root open>
					<Tooltip.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" size="sm" {...props}>Click me repeatedly</Button>
						{/snippet}
					</Tooltip.Trigger>
					<Tooltip.Content>
						Stays open on re-click (disableCloseOnTriggerClick=true)
					</Tooltip.Content>
				</Tooltip.Root>
			</TooltipProvider>
		</div>
	{/snippet}
</Story>

<Story
	name="EnabledInteraction"
	tags={['interactive']}
	args={{ onOpenChange: fn(), delayDuration: 0, disableHoverableContent: true }}
	play={async ({ args, canvasElement }) => {
		const canvas = within(canvasElement);

		const trigger = canvas.getByRole('button', { name: 'Hover me' });
		await userEvent.tab();

		await expect(trigger).toHaveFocus();
		await expect(args.onOpenChange).toHaveBeenCalledWith(true);

		await userEvent.keyboard('{Escape}');
		await expect(args.onOpenChange).toHaveBeenCalledWith(false);
	}}
>
	{#snippet template(args)}
		<div class="flex items-center justify-center p-16">
			<TooltipProvider
				delayDuration={args.delayDuration}
				disableHoverableContent={args.disableHoverableContent}
			>
				<TooltipPrimitive.Root onOpenChange={args.onOpenChange}>
					<TooltipPrimitive.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" size="sm" {...props}>Hover me</Button>
						{/snippet}
					</TooltipPrimitive.Trigger>
					<TooltipPrimitive.Content>Keyboard-focus opens this tooltip</TooltipPrimitive.Content>
				</TooltipPrimitive.Root>
			</TooltipProvider>
		</div>
	{/snippet}
</Story>

<Story
	name="DisabledInteraction"
	tags={['interactive']}
	args={{ onOpenChange: fn(), disabled: true, delayDuration: 0, disableHoverableContent: true }}
	play={async ({ args, canvasElement }) => {
		const canvas = within(canvasElement);

		const trigger = canvas.getByRole('button', { name: 'Blocked trigger' });
		await userEvent.tab();

		await expect(trigger).toHaveFocus();

		await expect(args.onOpenChange).not.toHaveBeenCalled();
	}}
>
	{#snippet template(args)}
		<div class="flex items-center justify-center p-16">
			<TooltipProvider
				disabled={args.disabled}
				delayDuration={args.delayDuration}
				disableHoverableContent={args.disableHoverableContent}
			>
				<TooltipPrimitive.Root onOpenChange={args.onOpenChange}>
					<TooltipPrimitive.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" size="sm" {...props}>Blocked trigger</Button>
						{/snippet}
					</TooltipPrimitive.Trigger>
					<TooltipPrimitive.Content>This content must not open</TooltipPrimitive.Content>
				</TooltipPrimitive.Root>
			</TooltipProvider>
		</div>
	{/snippet}
</Story>

<Story
	name="KeyboardFocusInteraction"
	tags={['interactive']}
	args={{
		onOpenChange: fn(),
		ignoreNonKeyboardFocus: true,
		delayDuration: 0,
		disableHoverableContent: true
	}}
	play={async ({ args, canvasElement }) => {
		const canvas = within(canvasElement);
		const trigger = canvas.getByRole('button', { name: 'Keyboard only' });

		await expect(trigger, 'trigger button should be in the DOM').toBeTruthy();

		await userEvent.tab();
		await expect(trigger).toHaveFocus();

		await waitFor(() => expect(args.onOpenChange).toHaveBeenCalledWith(true));

		await userEvent.keyboard('{Escape}');
		await expect(args.onOpenChange).toHaveBeenCalledWith(false);
	}}
>
	{#snippet template(args)}
		<div class="flex items-center justify-center p-16">
			<TooltipProvider
				ignoreNonKeyboardFocus={args.ignoreNonKeyboardFocus}
				delayDuration={args.delayDuration}
				disableHoverableContent={args.disableHoverableContent}
			>
				<TooltipPrimitive.Root onOpenChange={args.onOpenChange}>
					<TooltipPrimitive.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" size="sm" {...props}>Keyboard only</Button>
						{/snippet}
					</TooltipPrimitive.Trigger>
					<TooltipPrimitive.Content>Opens on keyboard focus only</TooltipPrimitive.Content>
				</TooltipPrimitive.Root>
			</TooltipProvider>
		</div>
	{/snippet}
</Story>

<Story
	name="AllVariantsSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'tooltip-provider/AllVariants');
	}}
>
	{#snippet template()}
		<div class="flex flex-col gap-8 p-8">
			<div class="flex items-center gap-4">
				<span class="w-48 shrink-0 text-xs text-[var(--text-2)]">Default (120ms)</span>
				<TooltipProvider delayDuration={120} disableHoverableContent={true}>
					<Tooltip.Root open>
						<Tooltip.Trigger>
							{#snippet child({ props })}
								<Button variant="outline" size="sm" {...props}>Action</Button>
							{/snippet}
						</Tooltip.Trigger>
						<Tooltip.Content>Default provider tooltip</Tooltip.Content>
					</Tooltip.Root>
				</TooltipProvider>
			</div>

			<div class="flex items-center gap-4">
				<span class="w-48 shrink-0 text-xs text-[var(--text-2)]">Delayed (800ms)</span>
				<TooltipProvider delayDuration={800} disableHoverableContent={true}>
					<Tooltip.Root open>
						<Tooltip.Trigger>
							{#snippet child({ props })}
								<Button variant="outline" size="sm" {...props}>Action</Button>
							{/snippet}
						</Tooltip.Trigger>
						<Tooltip.Content>Delayed tooltip</Tooltip.Content>
					</Tooltip.Root>
				</TooltipProvider>
			</div>

			<div class="flex items-center gap-4">
				<span class="w-48 shrink-0 text-xs text-[var(--text-2)]">Disabled</span>
				<TooltipProvider disabled={true} delayDuration={120} disableHoverableContent={true}>
					<Tooltip.Root open>
						<Tooltip.Trigger>
							{#snippet child({ props })}
								<Button variant="outline" size="sm" {...props}>Action</Button>
							{/snippet}
						</Tooltip.Trigger>
						<Tooltip.Content>Should not appear</Tooltip.Content>
					</Tooltip.Root>
				</TooltipProvider>
			</div>

			<div class="flex items-center gap-4">
				<span class="w-48 shrink-0 text-xs text-[var(--text-2)]">Hoverable content</span>
				<TooltipProvider delayDuration={120} disableHoverableContent={false}>
					<Tooltip.Root open>
						<Tooltip.Trigger>
							{#snippet child({ props })}
								<Button variant="outline" size="sm" {...props}>Action</Button>
							{/snippet}
						</Tooltip.Trigger>
						<Tooltip.Content>Content stays open on hover</Tooltip.Content>
					</Tooltip.Root>
				</TooltipProvider>
			</div>

			<div class="flex items-center gap-4">
				<span class="w-48 shrink-0 text-xs text-[var(--text-2)]">Keyboard focus only</span>
				<TooltipProvider
					ignoreNonKeyboardFocus={true}
					delayDuration={120}
					disableHoverableContent={true}
				>
					<Tooltip.Root open>
						<Tooltip.Trigger>
							{#snippet child({ props })}
								<Button variant="outline" size="sm" {...props}>Tab to me</Button>
							{/snippet}
						</Tooltip.Trigger>
						<Tooltip.Content
							>Only opens on keyboard focus (ignoreNonKeyboardFocus=true)</Tooltip.Content
						>
					</Tooltip.Root>
				</TooltipProvider>
			</div>

			<div class="flex items-center gap-4">
				<span class="w-48 shrink-0 text-xs text-[var(--text-2)]">No close on click</span>
				<TooltipProvider
					disableCloseOnTriggerClick={true}
					delayDuration={120}
					disableHoverableContent={true}
				>
					<Tooltip.Root open>
						<Tooltip.Trigger>
							{#snippet child({ props })}
								<Button variant="outline" size="sm" {...props}>Click me</Button>
							{/snippet}
						</Tooltip.Trigger>
						<Tooltip.Content
							>Stays open on re-click (disableCloseOnTriggerClick=true)</Tooltip.Content
						>
					</Tooltip.Root>
				</TooltipProvider>
			</div>
		</div>
	{/snippet}
</Story>
