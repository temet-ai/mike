<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { fn, expect, userEvent, within, waitFor } from 'storybook/test';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import * as Sheet from './index';
	import { Button } from '$lib/components/ui/button';
	import XIcon from '@lucide/svelte/icons/x';

	const { Story } = defineMeta({
		title: 'UI/Sheet/SheetClose',
		component: Sheet.Close,
		tags: ['autodocs']
	});

	const closesOnClickSpy = fn();
	const keyboardCloseSpy = fn();
</script>

<script lang="ts">
	let closesOnClickOpen = $state(true);
	let keyboardCloseOpen = $state(true);

	let refBindingEl = $state<HTMLButtonElement | null>(null);
</script>

<Story
	name="DefaultIdle"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'sheet-close/DefaultIdle');
	}}
>
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content aria-describedby={undefined}>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title>Example Sheet</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm">
							The close button (×) is positioned at the top-right corner.
						</p>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story name="ExplicitClose">
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content aria-describedby={undefined} showCloseButton={false}>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title>Custom Close</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm">
							Close button is supplied explicitly via Sheet.Close rather than the built-in
							SheetContent close button.
						</p>
						<Sheet.Close>
							{#snippet child({ props })}
								<Button variant="outline" size="sm" aria-label="Close sheet" {...props}>
									Dismiss
								</Button>
							{/snippet}
						</Sheet.Close>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="DisabledState"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		const btn = document.body.querySelector<HTMLButtonElement>(
			'[data-slot="sheet-close"] button, button[aria-label="Close"]'
		);
		await expect(btn, 'disabled close button should be in document.body (portaled)').toBeTruthy();
		await expect(
			btn?.hasAttribute('disabled') || btn?.getAttribute('data-disabled') !== null,
			'button should be disabled'
		).toBe(true);
		await captureStorySnapshot(canvasElement, 'sheet-close/DisabledState');
	}}
>
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content aria-describedby={undefined} showCloseButton={false}>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title>Disabled Close</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm">
							The close button is disabled — opacity-50, pointer-events-none.
						</p>
						<Sheet.Close disabled>
							{#snippet child({ props })}
								<Button variant="ghost" size="icon" aria-label="Close" {...props}>
									<XIcon class="size-4" aria-hidden="true" />
								</Button>
							{/snippet}
						</Sheet.Close>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="ClosesOnClick"
	tags={['interactive']}
	play={async () => {
		closesOnClickOpen = true;
		closesOnClickSpy.mockClear();

		const content = document.body.querySelector('[data-slot="sheet-content"]');
		await expect(content, 'sheet-content should be in the DOM when sheet is open').toBeTruthy();

		await expect(
			content?.getAttribute('data-state'),
			'sheet-content should have data-state="open" initially'
		).toBe('open');

		const body = within(document.body);
		const closeBtn = body.getByRole('button', { name: /close/i });
		await expect(closeBtn, 'close button should be rendered').toBeTruthy();

		await userEvent.click(closeBtn);

		await expect(closesOnClickSpy).toHaveBeenCalledWith(false);
	}}
>
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root
				bind:open={closesOnClickOpen}
				onOpenChange={(v) => {
					closesOnClickOpen = v;
					closesOnClickSpy(v);
				}}
			>
				<Sheet.Content aria-describedby={undefined}>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title>Click to Close</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm">Click the × button to dismiss this sheet.</p>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="KeyboardClose"
	tags={['interactive']}
	play={async () => {
		keyboardCloseOpen = true;
		keyboardCloseSpy.mockClear();

		const content = document.body.querySelector('[data-slot="sheet-content"]');
		await expect(content, 'sheet-content in DOM before keyboard close').toBeTruthy();

		await expect(
			content?.getAttribute('data-state'),
			'sheet-content should have data-state="open" initially'
		).toBe('open');

		if (!content) throw new Error('sheet-content not found');
		await Promise.all(content.getAnimations().map((animation) => animation.finished));

		const closeBtn = document.body.querySelector<HTMLButtonElement>('[data-slot="sheet-close"]');
		await expect(closeBtn, 'close button should be in document.body (portaled)').toBeTruthy();
		if (!closeBtn) throw new Error('sheet-close button not found');

		await waitFor(
			async () => {
				closeBtn.focus();
				expect(document.activeElement, 'close button should have focus').toBe(closeBtn);
				await userEvent.keyboard('{Enter}');
				expect(keyboardCloseSpy).toHaveBeenCalledWith(false);
			},
			{ timeout: 2_000, interval: 100 }
		);
	}}
>
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root
				bind:open={keyboardCloseOpen}
				onOpenChange={(v) => {
					keyboardCloseOpen = v;
					keyboardCloseSpy(v);
				}}
			>
				<Sheet.Content aria-describedby={undefined}>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title>Keyboard Close</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm">
							Focus the × button and press Enter to close via keyboard.
						</p>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="AllThemesSnapshot"
	tags={['visual']}
	play={async () => {
		await captureStorySnapshot(document.body as HTMLElement, 'sheet-close/AllThemes');
	}}
>
	{#snippet template()}
		<div class="flex flex-col gap-4 p-4 bg-[var(--surface-bg)]">
			<p class="text-[var(--text-2)] text-xs font-mono">
				Sheet.Close (ghost button) — all 5 themes
			</p>
			{#each ['forge', 'glacier', 'void', 'phosphor', 'crimson'] as theme (theme)}
				<div
					data-theme={theme}
					class="relative flex items-center gap-3 rounded-md border border-[var(--border-subtle)] bg-[var(--surface-card)] px-4 py-3 h-[60px] overflow-hidden"
				>
					<span class="text-[var(--text-2)] text-xs font-mono w-20">{theme}</span>
					<Sheet.Root open>
						<Sheet.Content aria-describedby={undefined} showCloseButton={false}>
							<Sheet.Title class="sr-only">{theme} theme close button</Sheet.Title>
							<Sheet.Close>
								{#snippet child({ props })}
									<Button
										variant="ghost"
										size="icon"
										aria-label="Close"
										class="relative"
										{...props}
									>
										<XIcon class="size-4" aria-hidden="true" />
									</Button>
								{/snippet}
							</Sheet.Close>
						</Sheet.Content>
					</Sheet.Root>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>

<Story
	name="FocusVisibleSnapshot"
	tags={['visual', 'interactive']}
	play={async () => {
		const btn = document.body.querySelector<HTMLButtonElement>(
			'[data-slot="sheet-close"] button, button[aria-label="Close"]'
		);
		await expect(btn, 'close button must be in document.body (portaled)').toBeTruthy();
		if (btn) {
			btn.focus();
			await expect(document.activeElement).toBe(btn);
		}
		await captureStorySnapshot(document.body as HTMLElement, 'sheet-close/FocusVisible');
	}}
>
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content aria-describedby={undefined}>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title>Focus Visible</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm">
							The close button receives a focus ring (ring-2, ring-[var(--ring)], ring-offset-2)
							when focused via keyboard.
						</p>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="RefBinding"
	tags={['interactive']}
	play={async () => {
		const btn = document.body.querySelector<HTMLButtonElement>(
			'[data-slot="sheet-close"] button, button[aria-label="Close"]'
		);
		await expect(btn, 'close button should be in document.body (portaled)').toBeTruthy();
		await expect(btn?.tagName.toLowerCase(), 'element should be a button').toBe('button');

		await expect(
			refBindingEl,
			'bind:ref on Sheet.Close should populate refBindingEl with the HTMLButtonElement'
		).toBe(btn);
		await expect(
			refBindingEl instanceof HTMLButtonElement,
			'refBindingEl should be an HTMLButtonElement instance'
		).toBe(true);
	}}
>
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content aria-describedby={undefined} showCloseButton={false}>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title>Ref Binding</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm">
							Sheet.Close exposes a ref $bindable prop. The play function asserts refBindingEl
							equals the portaled HTMLButtonElement after mount.
						</p>
						<Sheet.Close bind:ref={refBindingEl}>
							{#snippet child({ props })}
								<Button variant="ghost" size="icon" aria-label="Close" {...props}>
									<XIcon class="size-4" aria-hidden="true" />
								</Button>
							{/snippet}
						</Sheet.Close>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="TypeSubmit"
	tags={['interactive']}
	play={async () => {
		await waitFor(() => {
			const el = document.body.querySelector<HTMLButtonElement>('[data-slot="sheet-close"]');
			expect(el, 'close button should be rendered in document.body').toBeTruthy();
		});

		const btn = document.body.querySelector<HTMLButtonElement>('[data-slot="sheet-close"]');
		await expect(btn, 'close button should be rendered').toBeTruthy();
		await expect(
			btn?.getAttribute('type'),
			'type prop should be forwarded to the underlying button'
		).toBe('submit');
	}}
>
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content aria-describedby={undefined} showCloseButton={false}>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title>Type Submit</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm">
							Sheet.Close with type="submit" — the type prop is forwarded to the underlying button
							element via restProps.
						</p>
						<Sheet.Close type="submit">
							{#snippet child({ props })}
								<Button variant="ghost" size="icon" aria-label="Close" {...props}>
									<XIcon class="size-4" aria-hidden="true" />
								</Button>
							{/snippet}
						</Sheet.Close>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>
