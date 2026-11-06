<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { fn, expect, userEvent, within, waitFor } from 'storybook/test';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import * as Sheet from './index';
	import SheetPortal from './sheet-portal.svelte';
	import SheetOverlay from './sheet-overlay.svelte';

	let capturedRef: HTMLElement | null = $state(null);

	const refBindingOverlaySpy = fn();
	const clickDismissSpy = fn();
	const escDismissSpy = fn();
	const animateToggleSpy = fn();

	const { Story } = defineMeta({
		title: 'UI/Sheet/SheetOverlay',
		component: SheetOverlay,
		tags: ['autodocs'],
		argTypes: {
			class: { control: 'text', description: 'Additional Tailwind class merged via cn()' },
			forceMount: { control: 'boolean', description: 'Keep overlay in DOM even when closed' }
		}
	});
</script>

<script lang="ts">
	let closedStateOpen = $state(false);
	let animateToggleOpen = $state(true);
</script>

<Story name="OpenState">
	{#snippet template()}
		<div
			class="relative h-[300px] overflow-hidden bg-[var(--surface-card)] rounded-md border border-[var(--border-subtle)]"
		>
			<Sheet.Root open>
				<SheetOverlay />
			</Sheet.Root>
			<div class="absolute inset-0 flex items-center justify-center pointer-events-none">
				<span
					class="text-[var(--text-2)] text-xs font-mono px-2 py-1 bg-[var(--surface-2)] rounded"
				>
					overlay open — bg-[var(--surface-bg)]/70
				</span>
			</div>
		</div>
	{/snippet}
</Story>

<Story
	name="ClosedState"
	tags={['interactive']}
	play={async ({ canvasElement: _c }) => {
		const overlay = document.querySelector<HTMLElement>('[data-slot="sheet-overlay"]');
		await expect(overlay, 'overlay should be in the DOM (forceMount=true)').not.toBeNull();

		await expect(
			overlay!.getAttribute('data-state'),
			'data-state should be "closed" on the closed overlay'
		).toBe('closed');
		await expect(
			overlay!.getAttribute('data-state') !== 'open',
			'data-state should NOT be "open" on the closed overlay'
		).toBe(true);
	}}
>
	{#snippet template()}
		<div
			class="relative h-[300px] overflow-hidden bg-[var(--surface-card)] rounded-md border border-[var(--border-subtle)]"
		>
			<Sheet.Root bind:open={closedStateOpen}>
				<SheetPortal>
					<SheetOverlay forceMount />
				</SheetPortal>
				<Sheet.Content aria-describedby={undefined}>
					<Sheet.Title class="sr-only">Closed state demo</Sheet.Title>
				</Sheet.Content>
			</Sheet.Root>
			<div class="absolute inset-0 flex items-center justify-center">
				<span
					class="text-[var(--text-2)] text-xs font-mono px-2 py-1 bg-[var(--surface-2)] rounded"
				>
					overlay closed — forceMount, data-closed, opacity 0
				</span>
			</div>
		</div>
	{/snippet}
</Story>

<Story
	name="ClassOverride"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const overlay = canvasElement.querySelector<HTMLElement>('[data-slot="sheet-overlay"]');
		await expect(overlay, 'overlay should be in canvas (no portal)').not.toBeNull();

		await expect(
			overlay!.classList.contains('ring-4'),
			'ring-4 should be in the merged class list'
		).toBe(true);
		await expect(
			overlay!.classList.contains('ring-inset'),
			'ring-inset should be in the merged class list'
		).toBe(true);
	}}
>
	{#snippet template()}
		<div
			class="relative h-[300px] overflow-hidden bg-[var(--surface-card)] rounded-md border border-[var(--border-subtle)]"
		>
			<Sheet.Root open>
				<SheetOverlay class="ring-4 ring-inset ring-[var(--status-info-fg)]/40" />
			</Sheet.Root>
			<div class="absolute inset-0 flex items-center justify-center pointer-events-none">
				<span
					class="text-[var(--text-2)] text-xs font-mono px-2 py-1 bg-[var(--surface-2)] rounded"
				>
					class override — ring-4 ring-inset ring-[var(--status-info-fg)]/40
				</span>
			</div>
		</div>
	{/snippet}
</Story>

<Story
	name="RefBinding"
	tags={['interactive']}
	play={async () => {
		refBindingOverlaySpy.mockClear();

		await waitFor(() => {
			expect(
				capturedRef,
				'capturedRef should be non-null after mount (bind:ref wired)'
			).not.toBeNull();
		});

		await expect(
			capturedRef,
			'capturedRef should be non-null after mount (bind:ref wired)'
		).not.toBeNull();
		await expect(
			capturedRef instanceof HTMLElement,
			'capturedRef should be an HTMLElement (ref binding target)'
		).toBe(true);
		await expect(
			(capturedRef as HTMLElement).dataset.slot,
			'data-slot should be "sheet-overlay"'
		).toBe('sheet-overlay');

		await userEvent.click(capturedRef!, { pointerEventsCheck: 0 });

		await waitFor(() => expect(refBindingOverlaySpy).toHaveBeenCalledWith(false));
	}}
>
	{#snippet template()}
		<div
			class="relative h-[300px] overflow-hidden bg-[var(--surface-card)] rounded-md border border-[var(--border-subtle)]"
		>
			<Sheet.Root open={true} onOpenChange={refBindingOverlaySpy}>
				<SheetOverlay bind:ref={capturedRef} />
				<Sheet.Content aria-describedby={undefined}>
					<div class="p-5">
						<Sheet.Title>Ref Binding</Sheet.Title>
					</div>
				</Sheet.Content>
			</Sheet.Root>
			<div class="absolute inset-0 flex items-center justify-center pointer-events-none">
				<span
					class="text-[var(--text-2)] text-xs font-mono px-2 py-1 bg-[var(--surface-2)] rounded"
				>
					ref binding — capturedRef assigned on mount
				</span>
			</div>
		</div>
	{/snippet}
</Story>

<Story
	name="ClickDismiss"
	tags={['interactive']}
	play={async () => {
		clickDismissSpy.mockClear();

		await waitFor(() => {
			const el = document.querySelector('[data-slot="sheet-overlay"]');
			expect(el, 'overlay should be present in document.body').not.toBeNull();
		});

		const overlay = document.querySelector<HTMLElement>('[data-slot="sheet-overlay"]');
		await expect(overlay, 'overlay should be present before click').not.toBeNull();

		await userEvent.click(overlay!, { pointerEventsCheck: 0 });

		await waitFor(() => expect(clickDismissSpy).toHaveBeenCalledWith(false));
	}}
>
	{#snippet template()}
		<div
			class="relative h-[300px] overflow-hidden bg-[var(--surface-card)] rounded-md border border-[var(--border-subtle)]"
		>
			<Sheet.Root open={true} onOpenChange={clickDismissSpy}>
				<SheetPortal>
					<SheetOverlay />
				</SheetPortal>
				<Sheet.Content aria-describedby={undefined}>
					<div class="p-5">
						<Sheet.Title>Click outside to close</Sheet.Title>
					</div>
				</Sheet.Content>
			</Sheet.Root>
			<div class="absolute top-2 left-2 pointer-events-none">
				<span
					class="text-[var(--text-2)] text-xs font-mono px-2 py-1 bg-[var(--surface-2)] rounded"
				>
					click the overlay to dismiss
				</span>
			</div>
		</div>
	{/snippet}
</Story>

<Story
	name="EscDismiss"
	tags={['interactive']}
	play={async () => {
		const overlay = document.querySelector('[data-slot="sheet-overlay"]');
		await expect(overlay, 'overlay should be present before Escape').not.toBeNull();

		await userEvent.keyboard('{Escape}');

		await expect(escDismissSpy).toHaveBeenCalledWith(false);
	}}
>
	{#snippet template()}
		<div
			class="relative h-[300px] overflow-hidden bg-[var(--surface-card)] rounded-md border border-[var(--border-subtle)]"
		>
			<Sheet.Root open onOpenChange={escDismissSpy}>
				<SheetPortal>
					<SheetOverlay />
				</SheetPortal>
				<Sheet.Content aria-describedby={undefined}>
					<div class="p-5">
						<Sheet.Title>Press Escape to close</Sheet.Title>
					</div>
				</Sheet.Content>
			</Sheet.Root>
			<div class="absolute top-2 left-2 pointer-events-none">
				<span
					class="text-[var(--text-2)] text-xs font-mono px-2 py-1 bg-[var(--surface-2)] rounded"
				>
					press Escape to dismiss
				</span>
			</div>
		</div>
	{/snippet}
</Story>

<Story
	name="AnimateToggle"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		animateToggleOpen = true;
		animateToggleSpy.mockClear();

		const overlay = document.querySelector<HTMLElement>('[data-slot="sheet-overlay"]');
		await expect(overlay, 'overlay should be in DOM (forceMount)').not.toBeNull();

		await expect(
			overlay!.getAttribute('data-state'),
			'data-state should be "open" on the open overlay'
		).toBe('open');
		await expect(
			overlay!.getAttribute('data-state') !== 'closed',
			'data-state should NOT be "closed" on the open overlay'
		).toBe(true);

		const canvas = within(canvasElement);
		void canvas;
		const closeBtn = within(document.body).getByRole('button', { name: /close/i });
		await userEvent.click(closeBtn);

		await expect(animateToggleSpy).toHaveBeenCalledWith(false);

		await waitFor(() => {
			const overlayAfterClose = document.querySelector<HTMLElement>('[data-slot="sheet-overlay"]');
			expect(
				overlayAfterClose,
				'overlay should still be in DOM after close (forceMount)'
			).not.toBeNull();
			expect(
				overlayAfterClose!.getAttribute('data-state'),
				'data-state should be "closed" once sheet closes'
			).toBe('closed');
		});
	}}
>
	{#snippet template()}
		<div
			class="relative h-[300px] overflow-hidden bg-[var(--surface-card)] rounded-md border border-[var(--border-subtle)]"
		>
			<Sheet.Root bind:open={animateToggleOpen} onOpenChange={animateToggleSpy}>
				<SheetPortal>
					<SheetOverlay forceMount />
				</SheetPortal>
				<Sheet.Content aria-describedby={undefined}>
					<div class="p-5">
						<Sheet.Title>Animate Toggle</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm mt-2">
							Click × to trigger fade-out-0 on the overlay.
						</p>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="AsChild"
	tags={['interactive']}
	play={async () => {
		await waitFor(() => {
			const el = document.querySelector('[data-slot="sheet-overlay-custom"]');
			expect(el, 'custom asChild element should be in document.body after mount').not.toBeNull();
		});

		const overlay = document.querySelector<HTMLElement>('[data-slot="sheet-overlay-custom"]');
		await expect(overlay, 'custom asChild element should be in the DOM').not.toBeNull();
		await expect(
			overlay!.tagName.toLowerCase(),
			'asChild element should be a div (not the default bits-ui element)'
		).toBe('div');
		await expect(
			overlay!.tagName.toLowerCase() === 'div',
			'asChild element is present in DOM and is a div (proves PresenceLayer mounted it)'
		).toBe(true);
	}}
>
	{#snippet template()}
		<div
			class="relative h-[300px] overflow-hidden bg-[var(--surface-card)] rounded-md border border-[var(--border-subtle)]"
		>
			<Sheet.Root open={true}>
				<SheetPortal>
					<SheetOverlay>
						{#snippet child(props)}
							<div
								data-slot="sheet-overlay-custom"
								class="fixed inset-0 isolate z-50 bg-[var(--surface-bg)]/70 duration-100"
								{...props}
							></div>
						{/snippet}
					</SheetOverlay>
				</SheetPortal>
				<Sheet.Content aria-describedby={undefined}>
					<Sheet.Title class="sr-only">AsChild overlay demo</Sheet.Title>
				</Sheet.Content>
			</Sheet.Root>
			<div class="absolute inset-0 flex items-center justify-center pointer-events-none">
				<span
					class="text-[var(--text-2)] text-xs font-mono px-2 py-1 bg-[var(--surface-2)] rounded"
				>
					asChild — custom div renders via child snippet
				</span>
			</div>
		</div>
	{/snippet}
</Story>

<Story
	name="OpenStateSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'sheet-overlay/OpenState');
	}}
>
	{#snippet template()}
		<div
			class="relative h-[300px] overflow-hidden bg-[var(--surface-card)] rounded-md border border-[var(--border-subtle)]"
		>
			<Sheet.Root open>
				<SheetOverlay />
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="AllThemesSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'sheet-overlay/AllThemes');
	}}
>
	{#snippet template()}
		<div class="flex flex-col gap-3 p-4 bg-[var(--surface-bg)]">
			<p class="text-[var(--text-2)] text-xs font-mono">
				SheetOverlay — bg-[var(--surface-bg)]/70 across all 5 themes
			</p>
			{#each ['forge', 'glacier', 'void', 'phosphor', 'crimson'] as theme}
				<div
					data-theme={theme}
					class="relative h-[80px] overflow-hidden rounded-md border border-[var(--border-subtle)] bg-[var(--surface-card)]"
				>
					<Sheet.Root open>
						<SheetOverlay />
					</Sheet.Root>
					<span
						class="absolute bottom-1 left-2 text-[var(--text-strong)] text-[10px] font-mono pointer-events-none z-[51]"
					>
						{theme}
					</span>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>
