<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import * as Popover from './index';
	import PopoverTrigger from './popover-trigger.svelte';
	import { Button } from '../button';

	const { Story } = defineMeta({
		title: 'UI/Popover/PopoverTrigger',
		component: PopoverTrigger,
		tags: ['autodocs'],
		parameters: {
			docs: {
				description: {
					component:
						'Low-level trigger wrapper around bits-ui PopoverPrimitive.Trigger. ' +
						'Exposes a type prop (default "button"), a bindable ref, and forwards restProps ' +
						'(size, variant, disabled, class, etc.) through to the child via the bits-ui ' +
						'child snippet pattern. Must be used inside Popover.Root context. ' +
						'bits-ui manages aria-expanded and aria-haspopup automatically.'
				}
			}
		}
	});

	const clickToToggleOpenChangeSpy = fn();
	const keyboardEnterOpenChangeSpy = fn();
	const keyboardSpaceOpenChangeSpy = fn();
	const keyboardEscapeOpenChangeSpy = fn();
	const refBindableOpenChangeSpy = fn();
	const typeSubmitFormSubmitSpy = fn();
</script>

<Story name="Default">
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Popover.Root>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Open popover</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">
						Popover content renders here when the trigger is activated.
					</p>
					<div class="mt-3 flex justify-end">
						<Popover.Close>
							{#snippet child({ props })}
								<Button variant="ghost" size="sm" {...props}>Close</Button>
							{/snippet}
						</Popover.Close>
					</div>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="DefaultSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'popover-trigger/Default');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Popover.Root>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Open popover</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">Popover content.</p>
				</Popover.Content>
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
						<Button variant="outline" {...props}>Filter options</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-strong)] text-sm font-semibold">Filter options</p>
					<p class="mt-1 text-[var(--text-2)] text-xs">
						Choose how requests are filtered in the current session view.
					</p>
					<div class="mt-3 flex justify-end">
						<Popover.Close>
							{#snippet child({ props })}
								<Button variant="ghost" size="sm" {...props}>Close</Button>
							{/snippet}
						</Popover.Close>
					</div>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="OpenSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'popover-trigger/Open');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Filter options</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-strong)] text-sm font-semibold">Filter options</p>
					<p class="mt-1 text-[var(--text-2)] text-xs">
						Popover is open — trigger has aria-expanded=true.
					</p>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story name="WithForm" tags={['design-exploration']}>
	{#snippet template()}
		<div class="flex items-center justify-center p-20">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="ghost" size="icon" aria-label="Edit filter rule" {...props}>
							<svg
								class="size-4 text-[var(--text-2)]"
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
									d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"
								/>
							</svg>
						</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content class="w-[22rem]">
					<form class="space-y-3">
						<fieldset class="space-y-3 border-0 p-0">
							<legend class="text-[var(--text-strong)] text-sm font-semibold"
								>Edit filter rule</legend
							>
							<div class="space-y-1">
								<label for="trigger-story-rule-kind" class="text-[var(--text-2)] text-xs"
									>Rule kind</label
								>
								<select
									id="trigger-story-rule-kind"
									class="w-full rounded-md border border-[var(--border-default)] bg-[var(--surface-2)] px-2.5 py-1.5 text-[var(--text-1)] text-sm outline-none focus:ring-1 focus:ring-[var(--status-info-fg)]"
								>
									<option value="text_substring">Text substring</option>
									<option value="xml_tag">XML tag</option>
								</select>
							</div>
							<div class="space-y-1">
								<label for="trigger-story-rule-value" class="text-[var(--text-2)] text-xs"
									>Match value</label
								>
								<input
									id="trigger-story-rule-value"
									type="text"
									value="<parameter>"
									class="w-full rounded-md border border-[var(--border-default)] bg-[var(--surface-2)] px-2.5 py-1.5 font-mono text-[var(--text-1)] text-sm outline-none focus:ring-1 focus:ring-[var(--status-info-fg)]"
								/>
							</div>
						</fieldset>
						<div class="flex justify-end gap-2 pt-1">
							<Popover.Close>
								{#snippet child({ props })}
									<Button variant="ghost" size="sm" {...props}>Cancel</Button>
								{/snippet}
							</Popover.Close>
							<Button variant="default" size="sm" type="submit">Save</Button>
						</div>
					</form>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story name="Anchored" tags={['design-exploration']}>
	{#snippet template()}
		<div class="flex items-end justify-end p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>More options</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content sideOffset={8} align="end">
					<ul class="space-y-1">
						<li>
							<button
								class="w-full rounded px-2 py-1.5 text-left text-[var(--text-1)] text-sm hover:bg-[var(--surface-2)]"
								>Copy request ID</button
							>
						</li>
						<li>
							<button
								class="w-full rounded px-2 py-1.5 text-left text-[var(--text-1)] text-sm hover:bg-[var(--surface-2)]"
								>Export as JSON</button
							>
						</li>
						<li>
							<button
								class="w-full rounded px-2 py-1.5 text-left text-[var(--status-danger-fg)] text-sm hover:bg-[var(--surface-2)]"
								>Delete session</button
							>
						</li>
					</ul>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="Disabled"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const trigger = canvas.getByRole('button', { name: /open popover \(disabled\)/i });

		await expect(trigger).toBeDisabled();

		const content =
			canvasElement.querySelector('[data-slot="popover-content"]') ??
			document.querySelector('[data-slot="popover-content"]');
		await expect(
			content,
			'popover content must NOT be mounted when trigger is disabled'
		).toBeNull();
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Popover.Root>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" disabled {...props}>Open popover (disabled)</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">
						This should not appear when trigger is disabled.
					</p>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="DisabledSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'popover-trigger/Disabled');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Popover.Root>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" disabled {...props}>Open popover (disabled)</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">Should not appear.</p>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story name="GhostVariant" tags={['design-exploration']}>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Popover.Root>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="ghost" size="icon" aria-label="Options" {...props}>
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
									d="M12 5v.01M12 12v.01M12 19v.01M12 6a1 1 0 110-2 1 1 0 010 2zm0 7a1 1 0 110-2 1 1 0 010 2zm0 7a1 1 0 110-2 1 1 0 010 2z"
								/>
							</svg>
						</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">Options menu</p>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="ClickToToggle"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		clickToToggleOpenChangeSpy.mockClear();
		const canvas = within(canvasElement);
		const trigger = canvas.getByRole('button', { name: /click to toggle/i });

		const initialExpanded = trigger.getAttribute('aria-expanded');
		await expect(
			initialExpanded === 'false' || initialExpanded === null,
			'aria-expanded must be false or absent before first click'
		).toBe(true);

		await userEvent.click(trigger);

		await expect(
			trigger.getAttribute('aria-expanded'),
			'aria-expanded must be "true" after clicking trigger to open'
		).toBe('true');

		await userEvent.click(trigger);

		await waitFor(() => {
			const v = trigger.getAttribute('aria-expanded');
			expect(
				v === 'false' || v === null,
				'aria-expanded must return to false/absent after second click to close'
			).toBe(true);
		});

		await expect(clickToToggleOpenChangeSpy).toHaveBeenCalled();
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Popover.Root onOpenChange={clickToToggleOpenChangeSpy}>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Click to toggle</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">Popover opened by clicking the trigger.</p>
					<div class="mt-3 flex justify-end">
						<Popover.Close>
							{#snippet child({ props })}
								<Button variant="ghost" size="sm" {...props}>Close</Button>
							{/snippet}
						</Popover.Close>
					</div>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="KeyboardEnter"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		keyboardEnterOpenChangeSpy.mockClear();
		const canvas = within(canvasElement);
		const trigger = canvas.getByRole('button', { name: /keyboard enter/i });

		trigger.focus();
		await userEvent.keyboard('{Enter}');

		await expect(
			trigger.getAttribute('aria-expanded'),
			'aria-expanded must be "true" after Enter keypress'
		).toBe('true');

		await expect(keyboardEnterOpenChangeSpy).toHaveBeenCalledWith(true);
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Popover.Root onOpenChange={keyboardEnterOpenChangeSpy}>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Keyboard Enter</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">Opened with the Enter key.</p>
					<div class="mt-3 flex justify-end">
						<Popover.Close>
							{#snippet child({ props })}
								<Button variant="ghost" size="sm" {...props}>Close</Button>
							{/snippet}
						</Popover.Close>
					</div>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="KeyboardSpace"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		keyboardSpaceOpenChangeSpy.mockClear();
		const canvas = within(canvasElement);
		const trigger = canvas.getByRole('button', { name: /keyboard space/i });

		trigger.focus();
		await userEvent.keyboard(' ');

		await expect(
			trigger.getAttribute('aria-expanded'),
			'aria-expanded must be "true" after Space keypress'
		).toBe('true');

		await expect(keyboardSpaceOpenChangeSpy).toHaveBeenCalledWith(true);
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Popover.Root onOpenChange={keyboardSpaceOpenChangeSpy}>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Keyboard Space</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">Opened with the Space key.</p>
					<div class="mt-3 flex justify-end">
						<Popover.Close>
							{#snippet child({ props })}
								<Button variant="ghost" size="sm" {...props}>Close</Button>
							{/snippet}
						</Popover.Close>
					</div>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="KeyboardEscape"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		keyboardEscapeOpenChangeSpy.mockClear();
		const canvas = within(canvasElement);
		const trigger = canvas.getByRole('button', { name: /keyboard escape/i });

		await userEvent.click(trigger);
		await expect(
			trigger.getAttribute('aria-expanded'),
			'aria-expanded must be "true" before Escape'
		).toBe('true');

		await userEvent.keyboard('{Escape}');

		await waitFor(() => {
			const v = trigger.getAttribute('aria-expanded');
			expect(
				v === 'false' || v === null,
				'aria-expanded must return to false/absent after Escape'
			).toBe(true);
		});

		await expect(keyboardEscapeOpenChangeSpy).toHaveBeenCalledWith(false);
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Popover.Root onOpenChange={keyboardEscapeOpenChangeSpy}>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Keyboard Escape</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">Press Escape to close this popover.</p>
					<div class="mt-3 flex justify-end">
						<Popover.Close>
							{#snippet child({ props })}
								<Button variant="ghost" size="sm" {...props}>Close</Button>
							{/snippet}
						</Popover.Close>
					</div>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="DisabledNoClick"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const trigger = canvas.getByRole('button', { name: /disabled trigger/i });

		await expect(trigger).toBeDisabled();

		const expanded = trigger.getAttribute('aria-expanded');
		await expect(
			expanded === 'false' || expanded === null,
			'aria-expanded must remain false/absent when trigger is disabled'
		).toBe(true);

		const content =
			canvasElement.querySelector('[data-slot="popover-content"]') ??
			document.querySelector('[data-slot="popover-content"]');
		await expect(
			content,
			'popover content must NOT mount when trigger button is disabled'
		).toBeNull();
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Popover.Root>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" disabled {...props}>Disabled trigger</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">This should never appear.</p>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story name="DataSlotAttribute" tags={['design-exploration']}>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Popover.Root>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Check data-slot</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">Content</p>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="RefBindable"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		refBindableOpenChangeSpy.mockClear();
		const sentinel = canvasElement.querySelector('[data-testid="ref-sentinel"]');
		await expect(
			sentinel?.getAttribute('data-ref-bound'),
			'data-ref-bound must be "true" after mount — ref must be populated'
		).toBe('true');

		const trigger = canvasElement.querySelector('[data-slot="popover-trigger"]');
		await expect(trigger, 'trigger element must be in DOM').toBeTruthy();

		await userEvent.click(trigger as HTMLElement);
		await expect(
			refBindableOpenChangeSpy,
			'onOpenChange must be called after clicking the ref-bound trigger'
		).toHaveBeenCalledWith(true);
	}}
>
	{#snippet template()}
		{@const state = (() => {
			let triggerRef: HTMLElement | null = $state(null);
			return {
				get triggerRef() {
					return triggerRef;
				},
				set triggerRef(v) {
					triggerRef = v;
				}
			};
		})()}
		<div class="flex items-center justify-center p-8">
			<div
				data-testid="ref-sentinel"
				data-ref-bound={state.triggerRef != null ? 'true' : 'false'}
				class="sr-only"
			></div>
			<Popover.Root onOpenChange={refBindableOpenChangeSpy}>
				<Popover.Trigger bind:ref={state.triggerRef}>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Ref bindable test</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">Popover opened via a ref-bound trigger.</p>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="AriaExpanded"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const trigger = canvas.getByRole('button', { name: /aria expanded test/i });

		const initialExpanded = trigger.getAttribute('aria-expanded');
		await expect(
			initialExpanded === 'false' || initialExpanded === null,
			'aria-expanded must be false or absent when popover is closed'
		).toBe(true);

		await userEvent.click(trigger);

		await expect(
			trigger.getAttribute('aria-expanded'),
			'aria-expanded must be "true" when popover is open'
		).toBe('true');

		await userEvent.keyboard('{Escape}');

		await waitFor(() => {
			const v = trigger.getAttribute('aria-expanded');
			expect(
				v === 'false' || v === null,
				'aria-expanded must return to false/absent after Escape closes the popover'
			).toBe(true);
		});
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Popover.Root>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Aria expanded test</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">Popover is open.</p>
					<div class="mt-3 flex justify-end">
						<Popover.Close>
							{#snippet child({ props })}
								<Button variant="ghost" size="sm" {...props}>Close</Button>
							{/snippet}
						</Popover.Close>
					</div>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="TypeProp"
	tags={['design-exploration']}
	play={async ({ canvasElement }) => {
		const triggerEl = canvasElement.querySelector('[data-slot="popover-trigger"]');
		await expect(triggerEl, 'trigger element must be in DOM').toBeTruthy();

		const btn = triggerEl?.tagName === 'BUTTON' ? triggerEl : triggerEl?.querySelector('button');
		const typeAttr = btn?.getAttribute('type');
		await expect(
			typeAttr,
			'trigger button must have type="button" by default (no explicit prop)'
		).toBe('button');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<form onsubmit={(e) => e.preventDefault()}>
				<Popover.Root>
					<Popover.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" {...props}>Open (default type inside form)</Button>
						{/snippet}
					</Popover.Trigger>
					<Popover.Content>
						<p class="text-[var(--text-1)] text-sm">
							Trigger type="button" prevents accidental form submission.
						</p>
					</Popover.Content>
				</Popover.Root>
			</form>
		</div>
	{/snippet}
</Story>

<Story
	name="TypeSubmit"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		typeSubmitFormSubmitSpy.mockClear();
		const triggerEl = canvasElement.querySelector('[data-slot="popover-trigger"]');
		await expect(triggerEl, 'trigger element must be in DOM').toBeTruthy();
		const btn = triggerEl?.tagName === 'BUTTON' ? triggerEl : triggerEl?.querySelector('button');
		await expect(
			btn?.getAttribute('type'),
			'trigger button must have type="submit" when prop is set'
		).toBe('submit');

		await userEvent.click(btn as HTMLElement);
		await expect(
			typeSubmitFormSubmitSpy,
			'form onsubmit must be called when a type="submit" trigger is clicked'
		).toHaveBeenCalled();
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<form
				onsubmit={(e) => {
					e.preventDefault();
					typeSubmitFormSubmitSpy(e);
				}}
			>
				<Popover.Root>
					<Popover.Trigger type="submit">
						{#snippet child({ props })}
							<Button variant="outline" {...props}>Submit (type="submit" override)</Button>
						{/snippet}
					</Popover.Trigger>
					<Popover.Content>
						<p class="text-[var(--text-1)] text-sm">
							This trigger has type="submit" — clicking it submits the form.
						</p>
					</Popover.Content>
				</Popover.Root>
			</form>
		</div>
	{/snippet}
</Story>

<Story
	name="FocusRing"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const trigger = canvas.getByRole('button', { name: /focus ring test/i });

		await userEvent.tab();

		await expect(document.activeElement, 'trigger button must receive focus via Tab').toBe(trigger);
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Popover.Root>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Focus ring test</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">Opened after keyboard focus.</p>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="TabNavigation"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const trigger = canvas.getByRole('button', { name: /tab navigation/i });

		trigger.focus();
		await expect(document.activeElement, 'trigger must receive keyboard focus').toBe(trigger);

		await userEvent.keyboard('{Enter}');

		await expect(
			trigger.getAttribute('aria-expanded'),
			'aria-expanded must be "true" after Enter'
		).toBe('true');

		await userEvent.tab();

		const content =
			canvasElement.querySelector('[data-slot="popover-content"]') ??
			document.querySelector('[data-slot="popover-content"]');
		await expect(content, 'popover content must be mounted to accept focus').toBeTruthy();
		await expect(
			content?.contains(document.activeElement),
			'focus must land inside popover content after Tab (not body/chrome)'
		).toBe(true);
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<Popover.Root>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Tab navigation</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">Focus moves here after Tab from trigger.</p>
					<div class="mt-3 flex justify-end">
						<Popover.Close>
							{#snippet child({ props })}
								<Button variant="ghost" size="sm" {...props}>Close</Button>
							{/snippet}
						</Popover.Close>
					</div>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story name="HoverState" tags={['design-exploration']}>
	{#snippet template()}
		<div class="flex items-center gap-8 justify-center p-8">
			<div class="flex flex-col items-center gap-2">
				<p class="text-[var(--text-2)] text-xs">Default</p>
				<Popover.Root>
					<Popover.Trigger>
						{#snippet child({ props })}
							<Button variant="outline" {...props}>Outline trigger</Button>
						{/snippet}
					</Popover.Trigger>
					<Popover.Content>
						<p class="text-[var(--text-1)] text-sm">Content</p>
					</Popover.Content>
				</Popover.Root>
			</div>
			<div class="flex flex-col items-center gap-2">
				<p class="text-[var(--text-2)] text-xs">Ghost</p>
				<Popover.Root>
					<Popover.Trigger>
						{#snippet child({ props })}
							<Button variant="ghost" {...props}>Ghost trigger</Button>
						{/snippet}
					</Popover.Trigger>
					<Popover.Content>
						<p class="text-[var(--text-1)] text-sm">Content</p>
					</Popover.Content>
				</Popover.Root>
			</div>
			<div class="flex flex-col items-center gap-2">
				<p class="text-[var(--text-2)] text-xs">Ghost icon</p>
				<Popover.Root>
					<Popover.Trigger>
						{#snippet child({ props })}
							<Button variant="ghost" size="icon" aria-label="Options menu" {...props}>
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
										d="M12 5v.01M12 12v.01M12 19v.01M12 6a1 1 0 110-2 1 1 0 010 2zm0 7a1 1 0 110-2 1 1 0 010 2zm0 7a1 1 0 110-2 1 1 0 010 2z"
									/>
								</svg>
							</Button>
						{/snippet}
					</Popover.Trigger>
					<Popover.Content>
						<p class="text-[var(--text-1)] text-sm">Options</p>
					</Popover.Content>
				</Popover.Root>
			</div>
		</div>
	{/snippet}
</Story>
