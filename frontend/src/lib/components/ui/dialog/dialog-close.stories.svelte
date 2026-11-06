<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import * as Dialog from './index';
	import { Button } from '$lib/components/ui/button';
	import XIcon from '@lucide/svelte/icons/x';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import DialogClose from './dialog-close.svelte';

	const { Story } = defineMeta({
		title: 'UI/Dialog/DialogClose',
		component: DialogClose,
		tags: ['autodocs'],
		parameters: {
			backgrounds: { disable: true }
		}
	});

	let clickStoryOpen = $state(true);
	let enterStoryOpen = $state(true);
	let spaceStoryOpen = $state(true);
	let disabledStoryOpen = $state(true);

	const onClickClose = fn();
	const onEnterClose = fn();
	const onSpaceClose = fn();
	const onDisabledClose = fn();

	let closeRef = $state<HTMLElement | null>(null);
</script>

<Story name="Default">
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-sm">
				<Dialog.Header>
					<Dialog.Title>Dialog with default close</Dialog.Title>
				</Dialog.Header>
				<p class="text-muted-foreground text-sm">
					The close button (top-right X) is auto-rendered by Dialog.Content.
				</p>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="ExplicitInFooter">
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-sm">
				<Dialog.Header>
					<Dialog.Title>Explicit close button</Dialog.Title>
					<Dialog.Description>
						Dialog.Close wraps a Button in the footer via the child snippet pattern.
					</Dialog.Description>
				</Dialog.Header>
				<Dialog.Footer>
					<Dialog.Close>
						{#snippet child({ props })}
							<Button variant="outline" {...props}>Cancel</Button>
						{/snippet}
					</Dialog.Close>
					<Button variant="default">Confirm</Button>
				</Dialog.Footer>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="ExplicitIconClose">
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-sm" showCloseButton={false}>
				<Dialog.Header>
					<Dialog.Title>Explicit icon close</Dialog.Title>
				</Dialog.Header>
				<Dialog.Close>
					{#snippet child({ props })}
						<Button
							variant="ghost"
							size="icon"
							aria-label="Close"
							class="absolute top-3 right-3"
							{...props}
						>
							<XIcon class="size-4" aria-hidden="true" />
						</Button>
					{/snippet}
				</Dialog.Close>
				<p class="text-muted-foreground text-sm">Close button via explicit Dialog.Close.</p>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="DisabledClose">
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-sm" showCloseButton={false}>
				<Dialog.Header>
					<Dialog.Title>Disabled close button</Dialog.Title>
				</Dialog.Header>
				<Dialog.Footer>
					<Dialog.Close>
						{#snippet child({ props })}
							<Button variant="outline" {...props} disabled>Close (disabled)</Button>
						{/snippet}
					</Dialog.Close>
				</Dialog.Footer>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="CustomContent">
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-sm">
				<Dialog.Header>
					<Dialog.Title>Custom close label</Dialog.Title>
				</Dialog.Header>
				<Dialog.Footer>
					<Dialog.Close>
						{#snippet child({ props })}
							<Button variant="outline" {...props}>Dismiss this dialog</Button>
						{/snippet}
					</Dialog.Close>
				</Dialog.Footer>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="AllThemes" tags={['design-exploration']}>
	{#snippet template()}
		<div class="flex flex-wrap gap-4 p-4">
			{#each ['forge', 'glacier', 'void', 'phosphor', 'crimson'] as theme (theme)}
				<div data-theme={theme} class="rounded-lg border border-border bg-background p-4">
					<p class="text-foreground mb-3 text-xs font-medium capitalize">{theme}</p>
					<Dialog.Root open>
						<Dialog.Content class="sm:max-w-40" showCloseButton={true}>
							<Dialog.Header>
								<Dialog.Title class="text-xs">Close ({theme})</Dialog.Title>
							</Dialog.Header>
						</Dialog.Content>
					</Dialog.Root>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>

<Story
	name="AllThemesSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(document.body, 'dialog-close/AllThemesSnapshot', {
			selector: '[data-slot="dialog-content"]'
		});
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-sm" showCloseButton={true}>
				<Dialog.Header>
					<Dialog.Title>Theme snapshot</Dialog.Title>
					<Dialog.Description>Visual baseline for dialog close button theming.</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="DataSlotAttribute"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const closeBtn = document.body.querySelector('[data-slot="dialog-close"]');
		await expect(closeBtn, 'dialog-close should have data-slot="dialog-close"').not.toBeNull();
		await expect(
			closeBtn?.getAttribute('data-slot'),
			'data-slot attribute value must be "dialog-close"'
		).toBe('dialog-close');
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-sm">
				<Dialog.Header>
					<Dialog.Title>Data slot check</Dialog.Title>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="DataStateAttribute"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const closeBtn = document.body.querySelector('[data-slot="dialog-close"]');
		await expect(closeBtn, 'close button should exist').not.toBeNull();
		const dataState = closeBtn?.getAttribute('data-state');
		await expect(
			dataState,
			`data-state should be "open" while dialog is open, got "${dataState}"`
		).toBe('open');
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-sm">
				<Dialog.Header>
					<Dialog.Title>Data state check</Dialog.Title>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="AccessibleName"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const body = within(document.body);
		const closeBtn = body.getByRole('button', { name: /close/i });
		await expect(
			closeBtn,
			'close button must be findable by accessible name "Close"'
		).not.toBeNull();
		await expect(
			closeBtn.getAttribute('aria-label') ?? closeBtn.textContent?.trim(),
			'accessible name must be "Close"'
		).toMatch(/close/i);
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-sm">
				<Dialog.Header>
					<Dialog.Title>Accessible close button</Dialog.Title>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="TypePropPassthrough"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const closeBtn = document.body.querySelector('[data-slot="dialog-close"]');
		await expect(closeBtn, 'close button should exist').not.toBeNull();
		await expect(
			closeBtn?.getAttribute('type'),
			'type attribute should be "button" (the default)'
		).toBe('button');
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-sm">
				<Dialog.Header>
					<Dialog.Title>Type prop passthrough (default)</Dialog.Title>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="TypePropSubmit"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const closeBtn = document.body.querySelector('[data-slot="dialog-close"]');
		await expect(closeBtn, 'close button should exist').not.toBeNull();
		await expect(
			closeBtn?.getAttribute('type'),
			'type attribute should be "submit" when type="submit" is passed'
		).toBe('submit');
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-sm" showCloseButton={false}>
				<Dialog.Header>
					<Dialog.Title>Type prop: submit</Dialog.Title>
				</Dialog.Header>
				<Dialog.Close type="submit">
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Submit-type close</Button>
					{/snippet}
				</Dialog.Close>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="TypePropReset"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const closeBtn = document.body.querySelector('[data-slot="dialog-close"]');
		await expect(closeBtn, 'close button should exist').not.toBeNull();
		await expect(
			closeBtn?.getAttribute('type'),
			'type attribute should be "reset" when type="reset" is passed'
		).toBe('reset');
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-sm" showCloseButton={false}>
				<Dialog.Header>
					<Dialog.Title>Type prop: reset</Dialog.Title>
				</Dialog.Header>
				<Dialog.Close type="reset">
					{#snippet child({ props })}
						<Button variant="outline" {...props}>Reset-type close</Button>
					{/snippet}
				</Dialog.Close>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="DisabledPropPassthrough"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		disabledStoryOpen = true;
		onDisabledClose.mockClear();

		const bodyWithin = within(document.body);
		const closeBtn = bodyWithin.getByRole('button', { name: /close \(disabled\)/i });
		await expect(closeBtn, 'disabled close button should be in DOM').not.toBeNull();
		await expect(
			closeBtn.hasAttribute('disabled'),
			'close button should have disabled attribute'
		).toBe(true);

		await expect(
			onDisabledClose,
			'a disabled close button must not fire onOpenChange'
		).not.toHaveBeenCalled();
	}}
>
	{#snippet template()}
		<Dialog.Root bind:open={disabledStoryOpen} onOpenChange={onDisabledClose}>
			<Dialog.Content class="sm:max-w-sm" showCloseButton={false}>
				<Dialog.Header>
					<Dialog.Title>Disabled passthrough</Dialog.Title>
				</Dialog.Header>
				<Dialog.Footer>
					<Dialog.Close>
						{#snippet child({ props })}
							<Button variant="outline" {...props} disabled>Close (disabled)</Button>
						{/snippet}
					</Dialog.Close>
				</Dialog.Footer>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="KeyboardFocusTabIndex"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const closeBtn = document.body.querySelector(
			'[data-slot="dialog-close"]'
		) as HTMLElement | null;
		await expect(closeBtn, 'close button should exist').not.toBeNull();
		const tabIndex = closeBtn?.tabIndex ?? -1;
		await expect(
			tabIndex,
			`close button tabIndex should be >= 0 (keyboard reachable), got ${tabIndex}`
		).toBeGreaterThanOrEqual(0);
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-sm">
				<Dialog.Header>
					<Dialog.Title>Keyboard focus</Dialog.Title>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="ClickClosesDialog"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		clickStoryOpen = true;
		onClickClose.mockClear();

		const closeBefore = document.body.querySelector('[data-slot="dialog-close"]');
		await expect(closeBefore, 'close button should be in DOM before click').not.toBeNull();
		await expect(closeBefore?.getAttribute('data-state')).toBe('open');

		await userEvent.click(closeBefore as HTMLElement);

		await expect(onClickClose).toHaveBeenCalledWith(false);
	}}
>
	{#snippet template()}
		<Dialog.Root bind:open={clickStoryOpen} onOpenChange={onClickClose}>
			<Dialog.Content class="sm:max-w-sm">
				<Dialog.Header>
					<Dialog.Title>Click closes dialog</Dialog.Title>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="EnterKeyClosesDialog"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		enterStoryOpen = true;
		onEnterClose.mockClear();

		const closeBtn = document.body.querySelector(
			'[data-slot="dialog-close"]'
		) as HTMLElement | null;
		await expect(closeBtn, 'close button should exist before Enter').not.toBeNull();
		await expect(closeBtn?.getAttribute('data-state')).toBe('open');

		(closeBtn as HTMLElement).focus();
		await userEvent.keyboard('{Enter}');

		await expect(onEnterClose).toHaveBeenCalledWith(false);
	}}
>
	{#snippet template()}
		<Dialog.Root bind:open={enterStoryOpen} onOpenChange={onEnterClose}>
			<Dialog.Content class="sm:max-w-sm">
				<Dialog.Header>
					<Dialog.Title>Enter key closes dialog</Dialog.Title>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="SpaceKeyClosesDialog"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		spaceStoryOpen = true;
		onSpaceClose.mockClear();

		const closeBtn = document.body.querySelector(
			'[data-slot="dialog-close"]'
		) as HTMLElement | null;
		await expect(closeBtn, 'close button should exist before Space').not.toBeNull();
		await expect(closeBtn?.getAttribute('data-state')).toBe('open');

		(closeBtn as HTMLElement).focus();
		await userEvent.keyboard(' ');

		await expect(onSpaceClose).toHaveBeenCalledWith(false);
	}}
>
	{#snippet template()}
		<Dialog.Root bind:open={spaceStoryOpen} onOpenChange={onSpaceClose}>
			<Dialog.Content class="sm:max-w-sm">
				<Dialog.Header>
					<Dialog.Title>Space key closes dialog</Dialog.Title>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="RestPropsPassthrough"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const closeBtn = document.body.querySelector('[data-slot="dialog-close"]');
		await expect(closeBtn, 'close button should exist').not.toBeNull();
		await expect(
			closeBtn?.getAttribute('aria-describedby'),
			'aria-describedby should be passed through via restProps'
		).toBe('dialog-close-desc');
		await expect(
			closeBtn?.getAttribute('data-testid'),
			'data-testid should be passed through via restProps'
		).toBe('my-close-button');
	}}
>
	{#snippet template()}
		<div>
			<p id="dialog-close-desc" class="sr-only">Closes the dialog and discards changes.</p>
			<Dialog.Root open>
				<Dialog.Content class="sm:max-w-sm">
					<Dialog.Header>
						<Dialog.Title>restProps passthrough</Dialog.Title>
					</Dialog.Header>
					<Dialog.Close aria-describedby="dialog-close-desc" data-testid="my-close-button">
						{#snippet child({ props })}
							<Button variant="ghost" size="icon" aria-label="Close" {...props}>
								<XIcon class="size-4" aria-hidden="true" />
							</Button>
						{/snippet}
					</Dialog.Close>
				</Dialog.Content>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="RefBinding"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		await expect(closeRef, 'closeRef should be populated after render').not.toBeNull();
		await expect(
			closeRef instanceof HTMLButtonElement,
			'closeRef should be an HTMLButtonElement'
		).toBe(true);
		await expect(
			closeRef?.getAttribute('data-slot'),
			'ref element should have data-slot="dialog-close"'
		).toBe('dialog-close');
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-sm" showCloseButton={false}>
				<Dialog.Header>
					<Dialog.Title>Ref binding</Dialog.Title>
				</Dialog.Header>
				<Dialog.Close bind:ref={closeRef}>
					{#snippet child({ props })}
						<Button variant="ghost" size="icon" aria-label="Close" {...props}>
							<XIcon class="size-4" aria-hidden="true" />
						</Button>
					{/snippet}
				</Dialog.Close>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="DefaultVisualSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(document.body, 'dialog-close/Default', {
			selector: '[data-slot="dialog-content"]'
		});
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-sm">
				<Dialog.Header>
					<Dialog.Title>Visual snapshot</Dialog.Title>
					<Dialog.Description>Baseline for default close button rendering.</Dialog.Description>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="ExplicitFooterSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(document.body, 'dialog-close/ExplicitFooter', {
			selector: '[data-slot="dialog-content"]'
		});
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-sm">
				<Dialog.Header>
					<Dialog.Title>Explicit footer close</Dialog.Title>
				</Dialog.Header>
				<Dialog.Footer>
					<Dialog.Close>
						{#snippet child({ props })}
							<Button variant="outline" {...props}>Cancel</Button>
						{/snippet}
					</Dialog.Close>
				</Dialog.Footer>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="DisabledVisualSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(document.body, 'dialog-close/Disabled', {
			selector: '[data-slot="dialog-content"]'
		});
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Content class="sm:max-w-sm" showCloseButton={false}>
				<Dialog.Header>
					<Dialog.Title>Disabled close visual</Dialog.Title>
				</Dialog.Header>
				<Dialog.Footer>
					<Dialog.Close>
						{#snippet child({ props })}
							<Button variant="outline" {...props} disabled>Close (disabled)</Button>
						{/snippet}
					</Dialog.Close>
				</Dialog.Footer>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>
