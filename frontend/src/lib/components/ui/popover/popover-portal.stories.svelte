<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within, userEvent, waitFor, fn } from 'storybook/test';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import * as Popover from './index';
	import PopoverPortal from './popover-portal.svelte';
	import { Button } from '../button';

	const { Story } = defineMeta({
		title: 'UI/Popover/PopoverPortal',
		component: PopoverPortal,
		tags: ['autodocs'],
		parameters: {
			docs: {
				description: {
					component:
						'Portal wrapper for Popover.Content. Teleports children to document.body (or a custom target) via bits-ui Portal. Must be used inside Popover.Root. Disabled mode renders inline without teleportation.'
				}
			}
		}
	});

	const interactiveDisabledInlineOpenChangeSpy = fn();
	const interactiveCustomSelectorOpenChangeSpy = fn();
	const interactiveDataAttributesOpenChangeSpy = fn();
	const interactiveAriaAttributesOpenChangeSpy = fn();
	const interactiveEventsPortalActionSpy = fn();
	const interactiveChildrenSnippetOpenChangeSpy = fn();
</script>

<Story name="Default" tags={['design-exploration']}>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Popover.Root>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Open portal popover</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Portal>
					<Popover.Content>
						<p class="text-[var(--text-1)] text-sm">
							This content is portalled to document.body by default.
						</p>
						<div class="mt-3 flex justify-end">
							<Popover.Close>
								{#snippet child({ props })}
									<Button variant="ghost" size="sm" {...props}>Close</Button>
								{/snippet}
							</Popover.Close>
						</div>
					</Popover.Content>
				</Popover.Portal>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="DefaultSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'PopoverPortal/Default');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Popover.Root>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Open portal popover</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Portal>
					<Popover.Content>
						<p class="text-[var(--text-1)] text-sm">Portal content.</p>
					</Popover.Content>
				</Popover.Portal>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story name="Open">
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Portal target: body</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Portal>
					<Popover.Content>
						<p class="text-[var(--text-strong)] text-sm font-semibold">Portalled content</p>
						<p class="mt-1 text-[var(--text-2)] text-xs">
							This node lives in document.body, not inside the trigger's parent.
						</p>
						<div class="mt-3 flex justify-end">
							<Popover.Close>
								{#snippet child({ props })}
									<Button variant="ghost" size="sm" {...props}>Close</Button>
								{/snippet}
							</Popover.Close>
						</div>
					</Popover.Content>
				</Popover.Portal>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="OpenSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'PopoverPortal/Open');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Open portal</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Portal>
					<Popover.Content>
						<p class="text-[var(--text-strong)] text-sm font-semibold">Portalled content</p>
						<p class="mt-1 text-[var(--text-2)] text-xs">Renders in document.body.</p>
					</Popover.Content>
				</Popover.Portal>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story name="Disabled">
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Inline portal (disabled)</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Portal disabled>
					<Popover.Content>
						<p class="text-[var(--text-strong)] text-sm font-semibold">Inline content</p>
						<p class="mt-1 text-[var(--text-2)] text-xs">
							Portal is disabled — content renders in the local DOM tree, not document.body.
						</p>
					</Popover.Content>
				</Popover.Portal>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="DisabledSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'PopoverPortal/Disabled');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Inline portal (disabled)</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Portal disabled>
					<Popover.Content>
						<p class="text-[var(--text-strong)] text-sm font-semibold">Inline content</p>
					</Popover.Content>
				</Popover.Portal>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story name="CustomTargetSelector">
	{#snippet template()}
		<div class="flex flex-col gap-6 p-8">
			<div class="flex items-center justify-center">
				<Popover.Root open>
					<Popover.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" {...props}>Portal to #custom-target</Button>
						{/snippet}
					</Popover.Trigger>
					<Popover.Portal to="#popover-portal-stories-custom-target">
						<Popover.Content>
							<p class="text-[var(--text-strong)] text-sm font-semibold">Custom target</p>
							<p class="mt-1 text-[var(--text-2)] text-xs">
								This content was portalled to the div below
								(to="#popover-portal-stories-custom-target").
							</p>
						</Popover.Content>
					</Popover.Portal>
				</Popover.Root>
			</div>
			<div
				id="popover-portal-stories-custom-target"
				class="min-h-10 rounded border border-dashed border-[var(--border-default)] p-2"
				aria-label="Custom portal target"
			>
				<p class="text-[var(--text-2)] text-xs">
					Portal target (selector: #popover-portal-stories-custom-target)
				</p>
			</div>
		</div>
	{/snippet}
</Story>

<Story
	name="CustomTargetSelectorSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'PopoverPortal/CustomTargetSelector');
	}}
>
	{#snippet template()}
		<div class="flex flex-col gap-6 p-8">
			<div class="flex items-center justify-center">
				<Popover.Root open>
					<Popover.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" {...props}>Portal to #custom-target</Button>
						{/snippet}
					</Popover.Trigger>
					<Popover.Portal to="#popover-portal-stories-custom-target-snap">
						<Popover.Content>
							<p class="text-[var(--text-strong)] text-sm font-semibold">Custom target</p>
						</Popover.Content>
					</Popover.Portal>
				</Popover.Root>
			</div>
			<div
				id="popover-portal-stories-custom-target-snap"
				class="min-h-10 rounded border border-dashed border-[var(--border-default)] p-2"
				aria-label="Custom portal target"
			>
				<p class="text-[var(--text-2)] text-xs">Portal target</p>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="CustomTargetElement">
	{#snippet template()}
		<div class="flex flex-col gap-6 p-8">
			<div class="flex items-center justify-center">
				<Popover.Root open>
					<Popover.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" {...props}>Portal to Element ref</Button>
						{/snippet}
					</Popover.Trigger>
					<Popover.Portal to="#popover-portal-stories-el-target">
						<Popover.Content>
							<p class="text-[var(--text-strong)] text-sm font-semibold">Element ref target</p>
							<p class="mt-1 text-[var(--text-2)] text-xs">
								Content portalled to a specific DOM target element.
							</p>
						</Popover.Content>
					</Popover.Portal>
				</Popover.Root>
			</div>
			<div
				id="popover-portal-stories-el-target"
				class="min-h-10 rounded border border-dashed border-[var(--border-default)] p-2"
				aria-label="Element reference portal target"
			>
				<p class="text-[var(--text-2)] text-xs">Portal target (Element reference)</p>
			</div>
		</div>
	{/snippet}
</Story>

<Story
	name="CustomTargetElementSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'PopoverPortal/CustomTargetElement');
	}}
>
	{#snippet template()}
		<div class="flex flex-col gap-6 p-8">
			<div class="flex items-center justify-center">
				<Popover.Root open>
					<Popover.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" {...props}>Portal to Element ref</Button>
						{/snippet}
					</Popover.Trigger>
					<Popover.Portal to="#popover-portal-stories-el-target-snap">
						<Popover.Content>
							<p class="text-[var(--text-strong)] text-sm font-semibold">Element ref target</p>
						</Popover.Content>
					</Popover.Portal>
				</Popover.Root>
			</div>
			<div
				id="popover-portal-stories-el-target-snap"
				class="min-h-10 rounded border border-dashed border-[var(--border-default)] p-2"
				aria-label="Element reference portal target"
			>
				<p class="text-[var(--text-2)] text-xs">Portal target (Element reference)</p>
			</div>
		</div>
	{/snippet}
</Story>

<Story name="WithAriaAttributes">
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>ARIA-labelled portal</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Portal>
					<Popover.Content aria-label="Custom popover portal" aria-describedby="portal-desc">
						<p id="portal-desc" class="text-[var(--text-1)] text-sm">
							The portal wrapper carries aria-label and aria-describedby via restProps.
						</p>
					</Popover.Content>
				</Popover.Portal>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="WithAriaAttributesSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'PopoverPortal/WithAriaAttributes');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>ARIA-labelled portal</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Portal>
					<Popover.Content aria-label="Custom popover portal" aria-describedby="portal-desc-snap">
						<p id="portal-desc-snap" class="text-[var(--text-1)] text-sm">
							Portal with aria-label and aria-describedby.
						</p>
					</Popover.Content>
				</Popover.Portal>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story name="WithDataAttributes">
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Data-attr portal</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Portal>
					<Popover.Content data-testid="portal-wrapper" data-slot="popover-portal">
						<p class="text-[var(--text-1)] text-sm">
							Portal wrapper carries data-testid and data-slot attributes.
						</p>
					</Popover.Content>
				</Popover.Portal>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="WithDataAttributesSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'PopoverPortal/WithDataAttributes');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Data-attr portal</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Portal>
					<Popover.Content data-testid="portal-wrapper-snap" data-slot="popover-portal">
						<p class="text-[var(--text-1)] text-sm">
							Portal wrapper carries data-testid and data-slot attributes.
						</p>
					</Popover.Content>
				</Popover.Portal>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story name="WithInteractiveChildren">
	{#snippet template()}
		<div class="flex items-center justify-center p-20">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Interactive portal</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Portal>
					<Popover.Content class="w-[18rem]">
						<form class="space-y-3">
							<fieldset class="space-y-2 border-0 p-0">
								<legend class="text-[var(--text-strong)] text-sm font-semibold">
									Form inside portal
								</legend>
								<div class="space-y-1">
									<label for="portal-input" class="text-[var(--text-2)] text-xs">Name</label>
									<input
										id="portal-input"
										type="text"
										placeholder="Enter name"
										class="w-full rounded-md border border-[var(--border-default)] bg-[var(--surface-2)] px-2.5 py-1.5 text-[var(--text-1)] text-sm outline-none focus:ring-1 focus:ring-[var(--status-info-fg)]"
									/>
								</div>
							</fieldset>
							<div class="flex justify-end gap-2">
								<Popover.Close>
									{#snippet child({ props })}
										<Button variant="ghost" size="sm" {...props}>Cancel</Button>
									{/snippet}
								</Popover.Close>
								<Button variant="default" size="sm" type="submit">Submit</Button>
							</div>
						</form>
					</Popover.Content>
				</Popover.Portal>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="WithInteractiveChildrenSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'PopoverPortal/WithInteractiveChildren');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-20">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Interactive portal</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Portal>
					<Popover.Content class="w-[18rem]">
						<form class="space-y-3">
							<fieldset class="space-y-2 border-0 p-0">
								<legend class="text-[var(--text-strong)] text-sm font-semibold">
									Form inside portal
								</legend>
								<div class="space-y-1">
									<label for="portal-input-snap" class="text-[var(--text-2)] text-xs">Name</label>
									<input
										id="portal-input-snap"
										type="text"
										placeholder="Enter name"
										class="w-full rounded-md border border-[var(--border-default)] bg-[var(--surface-2)] px-2.5 py-1.5 text-[var(--text-1)] text-sm outline-none focus:ring-1 focus:ring-[var(--status-info-fg)]"
									/>
								</div>
							</fieldset>
						</form>
					</Popover.Content>
				</Popover.Portal>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story name="EmptyChildren">
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Empty portal</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Portal>
					<Popover.Content>
						<span class="text-[var(--text-2)] text-xs italic">(no content)</span>
					</Popover.Content>
				</Popover.Portal>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="EmptyChildrenSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'PopoverPortal/EmptyChildren');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Empty portal</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Portal>
					<Popover.Content>
						<span class="text-[var(--text-2)] text-xs italic">(no content)</span>
					</Popover.Content>
				</Popover.Portal>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="InteractivePortalToBody"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const trigger = canvas.getByRole('button', { name: /open portal/i });
		await userEvent.click(trigger);

		const contentInCanvas = canvasElement.querySelector('[data-slot="popover-content"]');
		await expect(
			contentInCanvas,
			'portalled content should NOT be inside canvasElement'
		).toBeNull();

		const contentInDoc = document.querySelector('[data-testid="portal-to-body-content"]');
		await expect(contentInDoc, 'portalled content should exist in document').not.toBeNull();

		await expect(contentInDoc?.textContent).toContain('Portalled to document.body');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Popover.Root>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Open portal popover</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Portal>
					<Popover.Content>
						<p data-testid="portal-to-body-content" class="text-[var(--text-1)] text-sm">
							Portalled to document.body.
						</p>
					</Popover.Content>
				</Popover.Portal>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="InteractiveDisabledInline"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		interactiveDisabledInlineOpenChangeSpy.mockClear();
		const canvas = within(canvasElement);

		const trigger = canvas.getByRole('button', { name: /inline portal/i });
		await userEvent.click(trigger);

		await expect(interactiveDisabledInlineOpenChangeSpy).toHaveBeenCalledWith(true);

		await waitFor(() => {
			const content = document.querySelector('[data-testid="interactive-disabled-inline-content"]');
			expect(
				content,
				'popover content should render in document after opening (via inner portal in Popover.Content)'
			).not.toBeNull();
		});

		const content = document.querySelector('[data-testid="interactive-disabled-inline-content"]');
		await expect(content?.textContent).toContain('no teleportation');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Popover.Root onOpenChange={interactiveDisabledInlineOpenChangeSpy}>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Inline portal (disabled)</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Portal disabled>
					<Popover.Content data-testid="interactive-disabled-inline-content">
						<p class="text-[var(--text-1)] text-sm">Inline content — no teleportation.</p>
					</Popover.Content>
				</Popover.Portal>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="InteractiveCustomSelectorTarget"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		interactiveCustomSelectorOpenChangeSpy.mockClear();
		const canvas = within(canvasElement);

		const trigger = canvas.getByRole('button', { name: /custom selector/i });
		await userEvent.click(trigger);

		await expect(interactiveCustomSelectorOpenChangeSpy).toHaveBeenCalledWith(true);

		const target = canvasElement.querySelector('#popover-portal-stories-ia-target');
		await expect(target, 'custom target element should exist in canvas DOM').not.toBeNull();

		const contentInDoc = document.querySelector('[data-testid="ia-selector-content"]');
		await expect(
			contentInDoc,
			'popover content should render somewhere in document with custom to= target'
		).not.toBeNull();

		await expect(contentInDoc?.textContent).toContain('Portalled to #popover');
	}}
>
	{#snippet template()}
		<div class="flex flex-col gap-6 p-8">
			<div class="flex items-center justify-center">
				<Popover.Root onOpenChange={interactiveCustomSelectorOpenChangeSpy}>
					<Popover.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" {...props}>Custom selector target</Button>
						{/snippet}
					</Popover.Trigger>
					<Popover.Portal to="#popover-portal-stories-ia-target">
						<Popover.Content>
							<p data-testid="ia-selector-content" class="text-[var(--text-1)] text-sm">
								Portalled to #popover-portal-stories-ia-target.
							</p>
						</Popover.Content>
					</Popover.Portal>
				</Popover.Root>
			</div>
			<div
				id="popover-portal-stories-ia-target"
				class="min-h-10 rounded border border-dashed border-[var(--border-default)] p-2"
				aria-label="Custom portal target"
			>
				<p class="text-[var(--text-2)] text-xs">Custom selector target</p>
			</div>
		</div>
	{/snippet}
</Story>

<Story
	name="InteractiveDataAttributes"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		interactiveDataAttributesOpenChangeSpy.mockClear();
		const canvas = within(canvasElement);

		const trigger = canvas.getByRole('button', { name: /data-attr portal/i });
		await userEvent.click(trigger);

		await expect(interactiveDataAttributesOpenChangeSpy).toHaveBeenCalledWith(true);

		const content = document.querySelector('[data-testid="data-attr-portal-content"]');
		await expect(
			content,
			'popover content should render when opened with data-* restProps on portal'
		).not.toBeNull();

		await expect(content?.textContent).toContain('data-* pass-through');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Popover.Root onOpenChange={interactiveDataAttributesOpenChangeSpy}>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Data-attr portal</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Portal>
					<Popover.Content data-testid="portal-data-attr-test" data-slot="popover-portal-test">
						<p data-testid="data-attr-portal-content" class="text-[var(--text-1)] text-sm">
							Portal with data-* pass-through.
						</p>
					</Popover.Content>
				</Popover.Portal>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="InteractiveAriaAttributes"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		interactiveAriaAttributesOpenChangeSpy.mockClear();
		const canvas = within(canvasElement);

		const trigger = canvas.getByRole('button', { name: /aria portal/i });
		await userEvent.click(trigger);

		await expect(interactiveAriaAttributesOpenChangeSpy).toHaveBeenCalledWith(true);

		const content = document.querySelector('[data-testid="aria-attr-portal-content"]');
		await expect(
			content,
			'popover content should render when opened with aria-* restProps on portal'
		).not.toBeNull();

		await expect(content?.textContent).toContain('aria-label and aria-describedby');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Popover.Root onOpenChange={interactiveAriaAttributesOpenChangeSpy}>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>ARIA portal</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Portal>
					<Popover.Content
						data-testid="portal-aria-test"
						aria-label="Accessibility test portal"
						aria-describedby="aria-test-desc"
					>
						<p
							id="aria-test-desc"
							data-testid="aria-attr-portal-content"
							class="text-[var(--text-1)] text-sm"
						>
							Portal with aria-label and aria-describedby via restProps.
						</p>
					</Popover.Content>
				</Popover.Portal>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="InteractiveEvents"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		interactiveEventsPortalActionSpy.mockClear();
		const portalledButton = document.querySelector(
			'[data-testid="portalled-action-btn"]'
		) as HTMLElement | null;
		await expect(
			portalledButton,
			'portalled action button should exist in document'
		).not.toBeNull();

		await userEvent.click(portalledButton as Element);

		await expect(interactiveEventsPortalActionSpy).toHaveBeenCalledOnce();
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Event portal</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Portal>
					<Popover.Content>
						<p class="mb-3 text-[var(--text-1)] text-sm">Click the button below:</p>
						<button
							data-testid="portalled-action-btn"
							onclick={interactiveEventsPortalActionSpy}
							class="w-full rounded-md border border-[var(--border-default)] bg-[var(--surface-2)] px-3 py-1.5 text-[var(--text-1)] text-sm"
						>
							Portalled action
						</button>
					</Popover.Content>
				</Popover.Portal>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="InteractiveEventsSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'PopoverPortal/InteractiveEvents');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Event portal</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Portal>
					<Popover.Content>
						<p class="mb-3 text-[var(--text-1)] text-sm">Click the button below:</p>
						<button
							data-testid="portalled-action-btn-snap"
							class="w-full rounded-md border border-[var(--border-default)] bg-[var(--surface-2)] px-3 py-1.5 text-[var(--text-1)] text-sm"
						>
							Portalled action
						</button>
					</Popover.Content>
				</Popover.Portal>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="InteractiveSlotForwarding"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const portalInput = document.getElementById('portal-slot-test-input');
		await expect(portalInput, 'portalled input should exist in document').not.toBeNull();

		await userEvent.click(portalInput as Element);

		await expect(document.activeElement?.id, 'portalled input should be focused after click').toBe(
			'portal-slot-test-input'
		);
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Slot forwarding portal</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Portal>
					<Popover.Content>
						<div class="space-y-2">
							<label for="portal-slot-test-input" class="text-[var(--text-2)] text-xs">
								Nested input
							</label>
							<input
								id="portal-slot-test-input"
								type="text"
								placeholder="Type here"
								class="w-full rounded-md border border-[var(--border-default)] bg-[var(--surface-2)] px-2.5 py-1.5 text-[var(--text-1)] text-sm outline-none focus:ring-1 focus:ring-[var(--status-info-fg)]"
							/>
						</div>
					</Popover.Content>
				</Popover.Portal>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="InteractiveChildrenSnippet"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		interactiveChildrenSnippetOpenChangeSpy.mockClear();
		const canvas = within(canvasElement);

		const trigger = canvas.getByRole('button', { name: /children snippet/i });
		await userEvent.click(trigger);

		await expect(interactiveChildrenSnippetOpenChangeSpy).toHaveBeenCalledWith(true);

		const snippetContent = document.querySelector('[data-testid="snippet-portal-content"]');
		await expect(
			snippetContent,
			'Snippet children should render in portal context after open'
		).not.toBeNull();
		await expect(
			snippetContent?.textContent?.trim(),
			'Snippet content text should be correct'
		).toBe('Snippet executed in portal');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Popover.Root onOpenChange={interactiveChildrenSnippetOpenChangeSpy}>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Children snippet portal</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Portal>
					<Popover.Content>
						<p data-testid="snippet-portal-content" class="text-[var(--text-1)] text-sm">
							Snippet executed in portal
						</p>
					</Popover.Content>
				</Popover.Portal>
			</Popover.Root>
		</div>
	{/snippet}
</Story>
