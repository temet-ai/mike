<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, waitFor, within } from 'storybook/test';
	import * as Tooltip from './index';
	import { Button } from '../button';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';

	const { Story } = defineMeta({
		title: 'UI/Tooltip/TooltipContent',
		component: Tooltip.Content,
		tags: ['autodocs'],
		argTypes: {
			side: {
				control: 'select',
				options: ['top', 'bottom', 'left', 'right'],
				description: 'Which side of the trigger the tooltip appears on'
			},
			align: {
				control: 'select',
				options: ['start', 'center', 'end'],
				description: 'Alignment of the tooltip relative to the trigger'
			},
			sideOffset: {
				control: { type: 'number', min: 0, max: 32, step: 1 },
				description: 'Gap in pixels between trigger and tooltip surface'
			}
		}
	});
</script>

<Story name="DefaultTop">
	{#snippet template()}
		<div class="flex items-center justify-center p-20">
			<Tooltip.Root open>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Hover me</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content>Default tooltip — top, center, offset 6</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story name="SideBottom" args={{ side: 'bottom' }}>
	{#snippet template(args)}
		<div class="flex items-start justify-center pb-20 pt-4">
			<Tooltip.Root open>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Hover me</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content side={args.side ?? 'bottom'}>
					Tooltip positioned below the trigger
				</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story name="SideLeft" args={{ side: 'left' }}>
	{#snippet template(args)}
		<div class="flex items-center justify-center p-20">
			<Tooltip.Root open>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Hover me</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content side={args.side ?? 'left'}>Left side tooltip</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story name="SideRight" args={{ side: 'right' }}>
	{#snippet template(args)}
		<div class="flex items-center justify-center p-20">
			<Tooltip.Root open>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Hover me</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content side={args.side ?? 'right'}>Right side tooltip</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story name="AlignStart" args={{ align: 'start' }}>
	{#snippet template(args)}
		<div class="flex items-center justify-center p-16">
			<Tooltip.Root open>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Start aligned</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content align={args.align ?? 'start'}>
					Aligned to the start edge of the trigger
				</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story name="AlignEnd" args={{ align: 'end' }}>
	{#snippet template(args)}
		<div class="flex items-center justify-center p-16">
			<Tooltip.Root open>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>End aligned</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content align={args.align ?? 'end'}>
					Aligned to the end edge of the trigger
				</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story name="OffsetCustom" args={{ sideOffset: 16 }}>
	{#snippet template(args)}
		<div class="flex items-center justify-center p-20">
			<Tooltip.Root open>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Custom offset</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content sideOffset={args.sideOffset ?? 16}>
					16px gap from trigger surface
				</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story name="LongText">
	{#snippet template()}
		<div class="flex items-center justify-center p-20">
			<Tooltip.Root open>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Long hint</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content>
					This tooltip contains a longer explanatory string that should wrap within the 280px
					maximum width constraint enforced by the component's default class list.
				</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="LongTextSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'tooltip-content/LongText');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-20">
			<Tooltip.Root open>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Long hint</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content>
					This tooltip contains a longer explanatory string that should wrap within the 280px
					maximum width constraint enforced by the component's default class list.
				</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="PortalDisabled"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		await waitFor(() => {
			expect(
				canvasElement.querySelector('[data-slot="tooltip-content"]'),
				'tooltip-content should mount inline in canvasElement'
			).not.toBeNull();
		});
		const tooltipContent = canvasElement.querySelector('[data-slot="tooltip-content"]')!;

		await expect(
			document.body.contains(tooltipContent),
			'with portal disabled, tooltip-content should be in the document'
		).toBe(true);

		await expect(
			canvasElement.contains(tooltipContent),
			'with portal disabled, tooltip-content should be inside canvasElement (not portalled out)'
		).toBe(true);

		await expect(
			tooltipContent.getAttribute('data-state'),
			'tooltip content should have data-state="instant-open" when tooltip is opened via controlled prop'
		).toBe('instant-open');

		await expect(
			tooltipContent.textContent?.trim(),
			'tooltip content should render the expected text when portal is disabled'
		).toBe('Inline (portal disabled)');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-20">
			<Tooltip.Root open delayDuration={0}>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Portal disabled</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content portalProps={{ disabled: true }}>Inline (portal disabled)</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="CustomClass"
	args={{ class: 'ring-2 ring-[var(--interactive-active-fg)]' }}
	tags={['interactive']}
	play={async () => {
		const tooltipContent = document.body.querySelector('[data-slot="tooltip-content"]');
		await expect(tooltipContent, 'tooltip-content element should be in the DOM').toBeTruthy();

		await expect(
			tooltipContent?.classList.contains('ring-2'),
			'ring-2 from the class prop should be present on [data-slot="tooltip-content"]'
		).toBe(true);

		await expect(
			tooltipContent?.classList.contains('ring-[var(--interactive-active-fg)]'),
			'ring-[var(--interactive-active-fg)] from the class prop should be present on [data-slot="tooltip-content"]'
		).toBe(true);
	}}
>
	{#snippet template(args)}
		<div class="flex items-center justify-center p-20">
			<Tooltip.Root open>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Custom styled</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content class={args.class}>Extra class applied via the class prop</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="CustomClassSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'tooltip-content/CustomClass');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-20">
			<Tooltip.Root open>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Custom styled</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content class="ring-2 ring-[var(--interactive-active-fg)]">
					Extra class applied via the class prop
				</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="RefBinding"
	tags={['interactive']}
	play={async () => {
		await waitFor(() => {
			expect(
				document.body.querySelector('[data-slot="tooltip-content"]'),
				'tooltip-content element should be in the DOM after mount'
			).not.toBeNull();
		});
		const tooltipContent = document.body.querySelector('[data-slot="tooltip-content"]')!;

		await expect(
			tooltipContent.getAttribute('data-state'),
			'tooltip content should have data-state="instant-open" when tooltip is opened via controlled prop'
		).toBe('instant-open');

		await expect(
			tooltipContent.textContent?.trim(),
			'tooltip content should render the expected text'
		).toBe('Ref is bound');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-20">
			<Tooltip.Root open delayDuration={0}>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Ref binding</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content>Ref is bound</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="ContentRendered"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await waitFor(() => {
			const all = Array.from(document.body.querySelectorAll('[data-slot="tooltip-content"]'));
			const el = all.find((e) => e.textContent?.trim() === 'Hint text rendered in the portal');
			expect(el, 'tooltip-content with expected text should be in document.body').not.toBeNull();
		});
		const tooltipContent = Array.from(
			document.body.querySelectorAll('[data-slot="tooltip-content"]')
		).find((e) => e.textContent?.trim() === 'Hint text rendered in the portal')!;

		await expect(
			tooltipContent.getAttribute('data-state'),
			'tooltip content should have data-state="instant-open" when opened via controlled prop'
		).toBe('instant-open');

		await expect(
			tooltipContent.textContent?.trim(),
			'tooltip content should render the exact hint string'
		).toBe('Hint text rendered in the portal');

		const trigger = canvas.getByRole('button', { name: 'Show hint' });
		await expect(
			trigger.getAttribute('aria-label') ?? trigger.textContent?.trim(),
			'trigger button should carry the "Show hint" label'
		).toBe('Show hint');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-20">
			<Tooltip.Root open delayDuration={0}>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" aria-label="Show hint" {...props}>Show hint</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content>Hint text rendered in the portal</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="AllSidesSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'tooltip-content/AllSides');
	}}
>
	{#snippet template()}
		<div class="grid grid-cols-2 gap-16 p-24">
			<div class="flex items-center justify-center">
				<Tooltip.Root open>
					<Tooltip.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" size="sm" {...props}>Top</Button>
						{/snippet}
					</Tooltip.Trigger>
					<Tooltip.Content side="top">side=top</Tooltip.Content>
				</Tooltip.Root>
			</div>
			<div class="flex items-end justify-center pb-2">
				<Tooltip.Root open>
					<Tooltip.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" size="sm" {...props}>Bottom</Button>
						{/snippet}
					</Tooltip.Trigger>
					<Tooltip.Content side="bottom">side=bottom</Tooltip.Content>
				</Tooltip.Root>
			</div>
			<div class="flex items-center justify-end pr-2">
				<Tooltip.Root open>
					<Tooltip.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" size="sm" {...props}>Left</Button>
						{/snippet}
					</Tooltip.Trigger>
					<Tooltip.Content side="left">side=left</Tooltip.Content>
				</Tooltip.Root>
			</div>
			<div class="flex items-center justify-start pl-2">
				<Tooltip.Root open>
					<Tooltip.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" size="sm" {...props}>Right</Button>
						{/snippet}
					</Tooltip.Trigger>
					<Tooltip.Content side="right">side=right</Tooltip.Content>
				</Tooltip.Root>
			</div>
		</div>
	{/snippet}
</Story>

<Story
	name="DefaultSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'tooltip-content/Default');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-20">
			<Tooltip.Root open>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Hover me</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content>Default tooltip surface</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>
