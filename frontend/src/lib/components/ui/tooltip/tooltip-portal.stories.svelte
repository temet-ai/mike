<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, waitFor, within } from 'storybook/test';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import * as Tooltip from './index';
	import { Button } from '../button';
	import TooltipPortal from './tooltip-portal.svelte';

	const { Story } = defineMeta({
		title: 'UI/Tooltip/TooltipPortal',
		component: TooltipPortal,
		tags: ['autodocs'],
		argTypes: {
			to: {
				control: 'text',
				description:
					'Portal destination. Accepts a CSS selector string or an Element. Defaults to document.body.'
			},
			disabled: {
				control: 'boolean',
				description: 'When true, renders inline without portalling.'
			}
		}
	});
</script>

<script lang="ts">
	let elementTargetEl: HTMLElement | null = $state(null);
</script>

<Story
	name="Default"
	args={{ disabled: false }}
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		await waitFor(() => {
			expect(
				document.body.querySelector('[data-slot="tooltip-content"]'),
				'portalled tooltip-content should be in document.body after mount'
			).not.toBeNull();
		});
		const tooltipContent = document.body.querySelector('[data-slot="tooltip-content"]')!;

		await expect(
			tooltipContent.textContent?.trim(),
			'portalled tooltip should contain the expected hint text'
		).toBe('Copy the API token to clipboard');

		await expect(
			canvasElement.contains(tooltipContent),
			'portalled tooltip content should NOT be inside canvasElement'
		).toBe(false);
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Tooltip.Root open delayDuration={0}>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Copy token</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content>Copy the API token to clipboard</Tooltip.Content>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="Disabled"
	args={{ disabled: true }}
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		await waitFor(() => {
			expect(
				canvasElement.querySelector('[data-slot="tooltip-content"]'),
				'with portalProps.disabled=true, tooltip-content should render inside canvasElement'
			).not.toBeNull();
		});
		const tooltipContent = canvasElement.querySelector('[data-slot="tooltip-content"]')!;

		await expect(
			canvasElement.contains(tooltipContent),
			'with portalProps.disabled=true, tooltip-content should be inside canvasElement (not portalled out)'
		).toBe(true);

		await expect(
			tooltipContent.textContent?.trim(),
			'tooltip content text should match the expected hint string'
		).toBe('Inline (no portal)');
	}}
>
	{#snippet template(args)}
		<div class="flex items-center justify-center p-16">
			<Tooltip.Root open delayDuration={0}>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Actions</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content portalProps={{ disabled: args.disabled }}
					>Inline (no portal)</Tooltip.Content
				>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="CustomTarget"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const target = canvasElement.querySelector('#tooltip-portal-target');
		await expect(
			target,
			'#tooltip-portal-target element should exist in canvasElement'
		).not.toBeNull();

		await waitFor(() => {
			expect(
				target?.querySelector('[data-slot="tooltip-content"]'),
				'tooltip-content should be inside the custom portal target'
			).not.toBeNull();
		});
		const tooltipContent = target!.querySelector('[data-slot="tooltip-content"]')!;

		await expect(
			tooltipContent.textContent?.trim(),
			'portalled tooltip should contain the expected hint text'
		).toBe('Portalled to custom target');
	}}
>
	{#snippet template()}
		<div class="flex flex-col items-center gap-8 p-16">
			<div id="tooltip-portal-target" data-testid="portal-target" class="min-h-4"></div>

			<Tooltip.Root open delayDuration={0}>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Custom target</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content portalProps={{ to: '#tooltip-portal-target' }}
					>Portalled to custom target</Tooltip.Content
				>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="ElementTarget"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const target = canvasElement.querySelector('[data-testid="element-portal-target"]');
		await expect(target, 'element portal target div should exist in the template').not.toBeNull();

		await waitFor(() => {
			expect(
				target?.querySelector('[data-slot="tooltip-content"]'),
				'tooltip-content should be portalled into the Element-ref target div'
			).not.toBeNull();
		});
		const tooltipContent = target!.querySelector('[data-slot="tooltip-content"]')!;

		await expect(
			tooltipContent.textContent?.trim(),
			'portalled tooltip should contain the expected hint text'
		).toBe('Portalled to element ref');
	}}
>
	{#snippet template()}
		<div class="flex flex-col items-center gap-8 p-16">
			<div bind:this={elementTargetEl} data-testid="element-portal-target" class="min-h-4"></div>

			<Tooltip.Root open delayDuration={0}>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Element ref target</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content portalProps={{ to: elementTargetEl ?? undefined }}
					>Portalled to element ref</Tooltip.Content
				>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="ElementTargetSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'tooltip-portal/ElementTarget');
	}}
>
	{#snippet template()}
		<div class="flex flex-col items-center gap-8 p-16">
			<div
				id="tooltip-portal-element-target-snap"
				data-testid="element-portal-target-snap"
				class="min-h-4"
			></div>

			<Tooltip.Root open>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Element ref target</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Portal to="#tooltip-portal-element-target-snap">
					<Tooltip.Content>Portalled to element ref</Tooltip.Content>
				</Tooltip.Portal>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="Snapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'tooltip-portal/Default');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Tooltip.Root open>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Copy token</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Portal>
					<Tooltip.Content>Copy the API token to clipboard</Tooltip.Content>
				</Tooltip.Portal>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="DisabledSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'tooltip-portal/Disabled');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Tooltip.Root open>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Actions</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Portal disabled>
					<Tooltip.Content>Inline (no portal)</Tooltip.Content>
				</Tooltip.Portal>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="CustomTargetSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'tooltip-portal/CustomTarget');
	}}
>
	{#snippet template()}
		<div class="flex flex-col items-center gap-8 p-16">
			<div id="tooltip-portal-target-snap" data-testid="portal-target-snap" class="min-h-4"></div>

			<Tooltip.Root open>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Custom target</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Portal to="#tooltip-portal-target-snap">
					<Tooltip.Content>Portalled to custom target</Tooltip.Content>
				</Tooltip.Portal>
			</Tooltip.Root>
		</div>
	{/snippet}
</Story>
