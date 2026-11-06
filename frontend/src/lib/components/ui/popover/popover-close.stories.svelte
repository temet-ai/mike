<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, waitFor } from 'storybook/test';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import * as Popover from './index';
	import { Button } from '../button';

	const { Story } = defineMeta({
		title: 'UI/Popover/PopoverClose',
		component: Popover.Close,
		tags: ['autodocs']
	});

	let bindableRefTarget = $state<HTMLElement | null>(null);

	const insideFormCancelDoesNotSubmitSpy = fn();
</script>

<Story name="Default">
	{#snippet template()}
		<div class="flex items-center justify-center p-12">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Open</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">Popover content</p>
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
		await captureStorySnapshot(canvasElement, 'popover-close/Default');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-12">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Open</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">Popover content</p>
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

<Story name="WithCustomAriaLabel">
	{#snippet template()}
		<div class="flex items-center justify-center p-12">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Open filter options</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-strong)] text-sm font-semibold">Filter options</p>
					<div class="mt-3 flex justify-end">
						<Popover.Close aria-label="Dismiss filter options">
							{#snippet child({ props })}
								<Button variant="ghost" size="sm" {...props}>
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
											d="M6 18L18 6M6 6l12 12"
										/>
									</svg>
								</Button>
							{/snippet}
						</Popover.Close>
					</div>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="WithCustomAriaLabelInteractive"
	tags={['interactive']}
	play={async () => {
		const closeEl = await waitFor(
			() => {
				const el = document.body.querySelector('[data-testid="with-custom-aria-label-close"]');
				if (!el) throw new Error('popover-close not yet in portal');
				return el;
			},
			{ timeout: 3000 }
		);

		await expect(closeEl, 'data-slot="popover-close" element must be in portal DOM').toBeTruthy();
		await expect(
			closeEl.getAttribute('aria-label'),
			'aria-label must be forwarded to the close element'
		).toBe('Dismiss filter options');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-12">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Open</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">Content</p>
					<div class="mt-3 flex justify-end">
						<Popover.Close
							aria-label="Dismiss filter options"
							data-testid="with-custom-aria-label-close"
						>
							{#snippet child({ props })}
								<Button variant="ghost" size="sm" {...props}>
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
											d="M6 18L18 6M6 6l12 12"
										/>
									</svg>
								</Button>
							{/snippet}
						</Popover.Close>
					</div>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story name="Disabled">
	{#snippet template()}
		<div class="flex items-center justify-center p-12">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Open</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">Close button is disabled in this example.</p>
					<div class="mt-3 flex justify-end">
						<Popover.Close disabled>
							{#snippet child({ props })}
								<Button variant="ghost" size="sm" disabled {...props}>Close (disabled)</Button>
							{/snippet}
						</Popover.Close>
					</div>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="DisabledSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'popover-close/Disabled');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-12">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Open</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">Close button is disabled in this example.</p>
					<div class="mt-3 flex justify-end">
						<Popover.Close disabled>
							{#snippet child({ props })}
								<Button variant="ghost" size="sm" disabled {...props}>Close (disabled)</Button>
							{/snippet}
						</Popover.Close>
					</div>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="DisabledInteractive"
	tags={['interactive']}
	play={async () => {
		const closeEl = await waitFor(
			() => {
				const el = document.body.querySelector('[data-testid="disabled-interactive-close"]');
				if (!el) throw new Error('popover-close not yet in portal');
				return el;
			},
			{ timeout: 3000 }
		);

		await expect(closeEl, 'close element must be in portal DOM').toBeTruthy();

		const disabledBtn =
			closeEl.querySelector('button[disabled]') ??
			(closeEl.hasAttribute('disabled') ? closeEl : null);
		await expect(
			disabledBtn,
			'disabled attribute must be present on close button or its child'
		).toBeTruthy();

		const content = document.body.querySelector('[data-testid="disabled-interactive-content"]');
		await expect(
			content,
			'popover content must remain mounted when close button is disabled'
		).toBeTruthy();
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-12">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Open</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content data-testid="disabled-interactive-content">
					<p class="text-[var(--text-1)] text-sm">Content remains visible.</p>
					<div class="mt-3 flex justify-end">
						<Popover.Close disabled data-testid="disabled-interactive-close">
							{#snippet child({ props })}
								<Button variant="ghost" size="sm" disabled {...props}>Close (disabled)</Button>
							{/snippet}
						</Popover.Close>
					</div>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="ClickToClose"
	tags={['interactive']}
	play={async () => {
		const contentBefore = await waitFor(
			() => {
				const el = document.body.querySelector('[data-testid="click-to-close-content"]');
				if (!el) throw new Error('popover-content not yet in portal');
				return el;
			},
			{ timeout: 3000 }
		);
		await expect(contentBefore, 'popover content must be mounted before click').toBeTruthy();

		const closeEl = document.body.querySelector('[data-testid="click-to-close-close"]');
		await expect(closeEl, 'close element must be in portal DOM').toBeTruthy();

		await userEvent.click(closeEl as Element);

		await waitFor(
			() => {
				const content = document.body.querySelector('[data-testid="click-to-close-content"]');
				if (content) throw new Error('popover content still mounted after close click');
			},
			{ timeout: 3000 }
		);

		const contentAfter = document.body.querySelector('[data-testid="click-to-close-content"]');
		await expect(contentAfter, 'popover content must be unmounted after clicking close').toBeNull();
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-12">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Open</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content data-testid="click-to-close-content">
					<p class="text-[var(--text-1)] text-sm">Click Close to dismiss.</p>
					<div class="mt-3 flex justify-end">
						<Popover.Close data-testid="click-to-close-close">
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
	name="KeyboardActivation"
	tags={['interactive']}
	play={async () => {
		await waitFor(
			() => {
				const el = document.body.querySelector('[data-testid="keyboard-activation-content"]');
				if (!el) throw new Error('popover-content not yet in portal');
			},
			{ timeout: 3000 }
		);

		const closeEl = document.body.querySelector('[data-testid="keyboard-activation-close"]');
		await expect(closeEl, 'close element must be in portal DOM').toBeTruthy();

		(closeEl as HTMLElement).focus();
		await userEvent.keyboard('{Enter}');

		await waitFor(
			() => {
				const content = document.body.querySelector('[data-testid="keyboard-activation-content"]');
				if (content) throw new Error('popover content still mounted after keyboard activation');
			},
			{ timeout: 3000 }
		);

		const contentAfter = document.body.querySelector('[data-testid="keyboard-activation-content"]');
		await expect(
			contentAfter,
			'popover content must be unmounted after keyboard activation'
		).toBeNull();
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-12">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Open</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content data-testid="keyboard-activation-content">
					<p class="text-[var(--text-1)] text-sm">Focus the close button and press Enter.</p>
					<div class="mt-3 flex justify-end">
						<Popover.Close data-testid="keyboard-activation-close">
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
	name="WithClickHandler"
	tags={['interactive']}
	args={{ onclick: fn() }}
	play={async ({ args }) => {
		const closeEl = await waitFor(
			() => {
				const el = document.body.querySelector('[data-testid="with-click-handler-close"]');
				if (!el) throw new Error('popover-close not yet in portal');
				return el;
			},
			{ timeout: 3000 }
		);

		await expect(closeEl, 'close element must be in portal DOM').toBeTruthy();

		await userEvent.click(closeEl);

		await expect(args.onclick, 'onclick spy must be called on click').toHaveBeenCalled();

		await waitFor(
			() => {
				const content = document.body.querySelector('[data-testid="with-click-handler-content"]');
				if (content) throw new Error('popover content still mounted after click');
			},
			{ timeout: 3000 }
		);

		const content = document.body.querySelector('[data-testid="with-click-handler-content"]');
		await expect(content, 'popover must close after clicking close button with handler').toBeNull();
	}}
>
	{#snippet template(args)}
		<div class="flex items-center justify-center p-12">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Open</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content data-testid="with-click-handler-content">
					<p class="text-[var(--text-1)] text-sm">Close button with onclick handler.</p>
					<div class="mt-3 flex justify-end">
						<Popover.Close onclick={args.onclick} data-testid="with-click-handler-close">
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
	name="WithCustomCssClasses"
	tags={['interactive']}
	play={async () => {
		const closeEl = await waitFor(
			() => {
				const el = document.body.querySelector('[data-slot="popover-close"]');
				if (!el) throw new Error('popover-close not yet in portal');
				return el;
			},
			{ timeout: 3000 }
		);

		await expect(closeEl, 'data-slot="popover-close" element must be in portal DOM').toBeTruthy();

		const classList = closeEl.className;
		await expect(
			classList.includes('opacity-70'),
			'forwarded class "opacity-70" must be present on [data-slot="popover-close"]'
		).toBe(true);
		await expect(
			classList.includes('transition-opacity'),
			'forwarded class "transition-opacity" must be present on [data-slot="popover-close"]'
		).toBe(true);
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-12">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Open</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">Close button with extra CSS classes.</p>
					<div class="mt-3 flex justify-end">
						<Popover.Close class="opacity-70 hover:opacity-100 transition-opacity">
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

<Story name="InsideForm">
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="ghost" size="icon" aria-label="Edit" {...props}>
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
									d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"
								/>
							</svg>
						</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content class="w-[20rem]">
					<form class="space-y-3">
						<fieldset class="space-y-3 border-0 p-0">
							<legend class="text-[var(--text-strong)] text-sm font-semibold">Edit rule</legend>
							<div class="space-y-1">
								<label for="close-story-rule-value" class="text-[var(--text-2)] text-xs"
									>Match value</label
								>
								<input
									id="close-story-rule-value"
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

<Story
	name="InsideFormSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'popover-close/InsideForm');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="ghost" size="icon" aria-label="Edit" {...props}>
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
									d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"
								/>
							</svg>
						</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content class="w-[20rem]">
					<form class="space-y-3">
						<fieldset class="space-y-3 border-0 p-0">
							<legend class="text-[var(--text-strong)] text-sm font-semibold">Edit rule</legend>
							<div class="space-y-1">
								<label for="close-story-rule-value-snapshot" class="text-[var(--text-2)] text-xs"
									>Match value</label
								>
								<input
									id="close-story-rule-value-snapshot"
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

<Story
	name="InsideFormCancelDoesNotSubmit"
	tags={['interactive']}
	play={async () => {
		insideFormCancelDoesNotSubmitSpy.mockClear();
		await waitFor(
			() => {
				const el = document.body.querySelector('[data-testid="inside-form-cancel-content"] form');
				if (!el) throw new Error('form inside popover-content not yet in portal');
			},
			{ timeout: 3000 }
		);

		const cancelEl = document.body.querySelector('[data-testid="inside-form-cancel-close"]');
		await expect(cancelEl, 'Cancel (Popover.Close) element must be in portal DOM').toBeTruthy();

		await userEvent.click(cancelEl as Element);

		await expect(
			insideFormCancelDoesNotSubmitSpy,
			'form must NOT be submitted when Cancel is clicked'
		).not.toHaveBeenCalled();

		await waitFor(
			() => {
				const content = document.body.querySelector('[data-testid="inside-form-cancel-content"]');
				if (content) throw new Error('popover content still mounted after cancel click');
			},
			{ timeout: 3000 }
		);

		const content = document.body.querySelector('[data-testid="inside-form-cancel-content"]');
		await expect(content, 'popover must close after clicking Cancel').toBeNull();
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-16">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="ghost" size="icon" aria-label="Edit" {...props}>
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
									d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"
								/>
							</svg>
						</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content class="w-[20rem]" data-testid="inside-form-cancel-content">
					<form
						class="space-y-3"
						onsubmit={(e) => {
							e.preventDefault();
							insideFormCancelDoesNotSubmitSpy(e);
						}}
					>
						<fieldset class="space-y-3 border-0 p-0">
							<legend class="text-[var(--text-strong)] text-sm font-semibold">Edit rule</legend>
							<div class="space-y-1">
								<label for="close-story-rule-value-interactive" class="text-[var(--text-2)] text-xs"
									>Match value</label
								>
								<input
									id="close-story-rule-value-interactive"
									type="text"
									value="<parameter>"
									class="w-full rounded-md border border-[var(--border-default)] bg-[var(--surface-2)] px-2.5 py-1.5 font-mono text-[var(--text-1)] text-sm outline-none focus:ring-1 focus:ring-[var(--status-info-fg)]"
								/>
							</div>
						</fieldset>
						<div class="flex justify-end gap-2 pt-1">
							<Popover.Close data-testid="inside-form-cancel-close">
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

<Story
	name="DataSlotAttribute"
	tags={['interactive']}
	play={async () => {
		const closeEl = await waitFor(
			() => {
				const el = document.body.querySelector('[data-slot="popover-close"]');
				if (!el) throw new Error('popover-close not yet in portal');
				return el;
			},
			{ timeout: 3000 }
		);

		await expect(
			closeEl,
			'data-slot="popover-close" element must exist in portal DOM'
		).toBeTruthy();

		await expect(
			closeEl.getAttribute('data-slot'),
			'data-slot attribute must equal "popover-close"'
		).toBe('popover-close');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-12">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Open</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">Checking data-slot attribute.</p>
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
	name="TypeAttribute"
	tags={['interactive']}
	play={async () => {
		const closeEl = await waitFor(
			() => {
				const el = document.body.querySelector('[data-slot="popover-close"]');
				if (!el) throw new Error('popover-close not yet in portal');
				return el;
			},
			{ timeout: 3000 }
		);

		await expect(closeEl, 'data-slot="popover-close" element must be in portal DOM').toBeTruthy();

		const btn = (
			closeEl.tagName === 'BUTTON' ? closeEl : closeEl.querySelector('button')
		) as HTMLButtonElement | null;
		await expect(btn, 'a <button> must be present inside the popover-close element').toBeTruthy();
		await expect(
			btn?.getAttribute('type'),
			'button must have type="button" (the default) to prevent accidental form submission'
		).toBe('button');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-12">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Open</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">Verifying type="button" default.</p>
					<div class="mt-3 flex justify-end">
						<Popover.Close type="button">
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
	name="BindableRef"
	tags={['interactive']}
	play={async () => {
		const closeEl = await waitFor(
			() => {
				const el = document.body.querySelector('[data-slot="popover-close"]');
				if (!el) throw new Error('popover-close not yet in portal');
				return el;
			},
			{ timeout: 3000 }
		);

		await expect(closeEl, 'data-slot="popover-close" element must be in portal DOM').toBeTruthy();

		await expect(
			bindableRefTarget,
			'bindableRefTarget must be populated by bind:ref after mount (not null)'
		).not.toBeNull();

		await expect(
			bindableRefTarget,
			'bind:ref must resolve to the same element as [data-slot="popover-close"] in the portal'
		).toBe(closeEl);

		await expect(
			bindableRefTarget instanceof HTMLElement,
			'bindableRefTarget must be an HTMLElement'
		).toBe(true);
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-12">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Open</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-1)] text-sm">
						ref binding — bind:ref exercises the $bindable prop.
					</p>
					<div class="mt-3 flex justify-end">
						<Popover.Close bind:ref={bindableRefTarget}>
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
	name="WithCustomAriaLabelSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'popover-close/WithCustomAriaLabel');
	}}
>
	{#snippet template()}
		<div class="flex items-center justify-center p-12">
			<Popover.Root open>
				<Popover.Trigger>
					{#snippet child({ props })}
						<Button variant="outline" size="sm" {...props}>Open filter options</Button>
					{/snippet}
				</Popover.Trigger>
				<Popover.Content>
					<p class="text-[var(--text-strong)] text-sm font-semibold">Filter options</p>
					<div class="mt-3 flex justify-end">
						<Popover.Close aria-label="Dismiss filter options">
							{#snippet child({ props })}
								<Button variant="ghost" size="sm" {...props}>
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
											d="M6 18L18 6M6 6l12 12"
										/>
									</svg>
								</Button>
							{/snippet}
						</Popover.Close>
					</div>
				</Popover.Content>
			</Popover.Root>
		</div>
	{/snippet}
</Story>
