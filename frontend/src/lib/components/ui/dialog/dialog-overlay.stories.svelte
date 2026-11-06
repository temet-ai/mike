<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { fn, expect, userEvent, waitFor } from 'storybook/test';
	import * as Dialog from './index';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import DialogOverlay from './dialog-overlay.svelte';

	const { Story } = defineMeta({
		title: 'UI/Dialog/DialogOverlay',
		component: DialogOverlay,
		tags: ['autodocs'],
		parameters: {
			backgrounds: { disable: true }
		}
	});

	const backdropClickSpy = fn();
	const escKeySpy = fn();
</script>

<Story name="OpenState" tags={['design-exploration']}>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Portal>
				<Dialog.Overlay />
			</Dialog.Portal>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="ClosedState" tags={['design-exploration']}>
	{#snippet template()}
		<div class="flex items-center justify-center p-8">
			<p class="text-foreground text-sm">
				Overlay in closed state (unmounted — dialog is not open).
			</p>
		</div>
		<Dialog.Root open={false}>
			<Dialog.Portal>
				<Dialog.Overlay />
			</Dialog.Portal>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="WithCustomClass" tags={['design-exploration']}>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Portal>
				<Dialog.Overlay class="ring-2 ring-inset ring-red-500/30" />
			</Dialog.Portal>
		</Dialog.Root>
	{/snippet}
</Story>

<Story name="ForgeTheme" tags={['design-exploration']}>
	{#snippet template()}
		<div data-theme="forge" class="bg-background min-h-32 w-full rounded-lg">
			<Dialog.Root open>
				<Dialog.Portal>
					<Dialog.Overlay />
				</Dialog.Portal>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story name="GlacierTheme" tags={['design-exploration']}>
	{#snippet template()}
		<div data-theme="glacier" class="bg-background min-h-32 w-full rounded-lg">
			<Dialog.Root open>
				<Dialog.Portal>
					<Dialog.Overlay />
				</Dialog.Portal>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story name="VoidTheme" tags={['design-exploration']}>
	{#snippet template()}
		<div data-theme="void" class="bg-background min-h-32 w-full rounded-lg">
			<Dialog.Root open>
				<Dialog.Portal>
					<Dialog.Overlay />
				</Dialog.Portal>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story name="PhosphorTheme" tags={['design-exploration']}>
	{#snippet template()}
		<div data-theme="phosphor" class="bg-background min-h-32 w-full rounded-lg">
			<Dialog.Root open>
				<Dialog.Portal>
					<Dialog.Overlay />
				</Dialog.Portal>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story name="CrimsonTheme" tags={['design-exploration']}>
	{#snippet template()}
		<div data-theme="crimson" class="bg-background min-h-32 w-full rounded-lg">
			<Dialog.Root open>
				<Dialog.Portal>
					<Dialog.Overlay />
				</Dialog.Portal>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="DataSlotAttribute"
	tags={['design-exploration']}
	play={async () => {
		const overlay = document.body.querySelector('[data-slot="dialog-overlay"]');
		await expect(
			overlay?.getAttribute('data-slot'),
			'overlay should carry data-slot="dialog-overlay"'
		).toBe('dialog-overlay');
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Portal>
				<Dialog.Overlay />
			</Dialog.Portal>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="DataOpenAttribute"
	tags={['design-exploration']}
	play={async () => {
		const overlay = document.body.querySelector('[data-slot="dialog-overlay"]');
		const hasDataOpen =
			overlay?.hasAttribute('data-open') || overlay?.getAttribute('data-state') === 'open';
		await expect(
			hasDataOpen,
			'overlay should have data-open or data-state="open" while dialog is open'
		).toBe(true);
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Portal>
				<Dialog.Overlay />
			</Dialog.Portal>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="ClassPropMerge"
	tags={['design-exploration']}
	play={async () => {
		const overlay = document.body.querySelector(
			'[data-slot="dialog-overlay"]'
		) as HTMLElement | null;
		await expect(overlay, 'overlay element should be in DOM').toBeTruthy();
		await expect(
			overlay?.classList.contains('sentinel-class-merge-test'),
			'custom class "sentinel-class-merge-test" should be present after cn() merge'
		).toBe(true);
		await expect(
			overlay?.classList.contains('z-50') || overlay?.classList.contains('fixed'),
			'base Tailwind class (z-50 or fixed) should remain after merge'
		).toBe(true);
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Portal>
				<Dialog.Overlay class="sentinel-class-merge-test" />
			</Dialog.Portal>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="RestPropsPassthrough"
	tags={['interactive']}
	args={{ 'data-testid': 'overlay-rest-test', id: 'overlay-rest-id' }}
	play={async ({ args }) => {
		const overlay = document.body.querySelector(
			'[data-testid="overlay-rest-test"]'
		) as HTMLElement | null;
		await expect(
			overlay,
			'overlay should be findable via data-testid passed through restProps'
		).toBeTruthy();
		await expect(
			overlay?.getAttribute('data-testid'),
			'data-testid restProp should equal the value passed in args'
		).toBe(args['data-testid']);
		await expect(
			overlay?.getAttribute('id'),
			'id restProp should equal the value passed in args'
		).toBe(args['id']);
		await expect(
			overlay?.getAttribute('data-slot'),
			'restProps element should still carry data-slot="dialog-overlay"'
		).toBe('dialog-overlay');
	}}
>
	{#snippet template(args)}
		<Dialog.Root open>
			<Dialog.Portal>
				<Dialog.Overlay data-testid={args['data-testid']} id={args['id']} />
			</Dialog.Portal>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="BackdropClickDismissal"
	tags={['interactive']}
	play={async () => {
		backdropClickSpy.mockClear();

		let overlayBefore: HTMLElement | null = null;
		await waitFor(() => {
			overlayBefore = document.body.querySelector(
				'[data-slot="dialog-overlay"]'
			) as HTMLElement | null;
			if (!overlayBefore) throw new Error('overlay not yet mounted in portal');
		});

		await expect(overlayBefore, 'overlay should be in DOM before backdrop click').toBeTruthy();

		await userEvent.click(overlayBefore!, { pointerEventsCheck: 0 });

		await waitFor(() => expect(backdropClickSpy).toHaveBeenCalledWith(false));
	}}
>
	{#snippet template()}
		<Dialog.Root open={true} onOpenChange={backdropClickSpy}>
			<Dialog.Portal>
				<Dialog.Overlay />
			</Dialog.Portal>
			<Dialog.Content class="sm:max-w-sm">
				<Dialog.Header>
					<Dialog.Title>Backdrop click test</Dialog.Title>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="EscKeyDismissal"
	tags={['interactive']}
	play={async () => {
		const overlayBefore = document.body.querySelector(
			'[data-slot="dialog-overlay"]'
		) as HTMLElement | null;
		await expect(overlayBefore, 'overlay should be in DOM before ESC').toBeTruthy();

		const content = document.body.querySelector(
			'[data-slot="dialog-content"]'
		) as HTMLElement | null;
		if (content) content.focus();

		await userEvent.keyboard('{Escape}');

		await expect(escKeySpy).toHaveBeenCalledWith(false);
	}}
>
	{#snippet template()}
		<Dialog.Root open onOpenChange={escKeySpy}>
			<Dialog.Portal>
				<Dialog.Overlay />
			</Dialog.Portal>
			<Dialog.Content class="sm:max-w-sm">
				<Dialog.Header>
					<Dialog.Title>ESC dismissal test</Dialog.Title>
				</Dialog.Header>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="ZIndexStacking"
	tags={['design-exploration']}
	play={async () => {
		const overlay = document.body.querySelector(
			'[data-slot="dialog-overlay"]'
		) as HTMLElement | null;
		await expect(overlay, 'overlay should exist').toBeTruthy();
		const style = window.getComputedStyle(overlay!);
		await expect(style.position, 'overlay position should be "fixed" to cover the viewport').toBe(
			'fixed'
		);
	}}
>
	{#snippet template()}
		<Dialog.Root open>
			<Dialog.Portal>
				<Dialog.Overlay />
			</Dialog.Portal>
		</Dialog.Root>
	{/snippet}
</Story>

<Story
	name="AllThemesSnapshot"
	tags={['visual']}
	play={async () => {
		await captureStorySnapshot(document.body as HTMLElement, 'dialog-overlay/AllThemes');
	}}
>
	{#snippet template()}
		<div class="flex flex-wrap gap-4 p-4">
			{#each ['forge', 'glacier', 'void', 'phosphor', 'crimson'] as theme (theme)}
				<div
					data-theme={theme}
					class="relative h-24 w-40 overflow-hidden rounded-lg border border-border bg-background"
				>
					<div class="absolute inset-0 flex items-center justify-center">
						<span class="text-foreground text-xs font-medium capitalize">{theme}</span>
					</div>
					<Dialog.Root open>
						<Dialog.Portal>
							<Dialog.Overlay />
						</Dialog.Portal>
					</Dialog.Root>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>

<Story
	name="OpenStateSnapshot"
	tags={['visual']}
	play={async () => {
		await captureStorySnapshot(document.body as HTMLElement, 'dialog-overlay/OpenState');
	}}
>
	{#snippet template()}
		<div class="relative h-48 w-full">
			<div class="flex h-full items-center justify-center bg-background">
				<p class="text-foreground text-sm">Page content behind the overlay</p>
			</div>
			<Dialog.Root open>
				<Dialog.Portal>
					<Dialog.Overlay />
				</Dialog.Portal>
			</Dialog.Root>
		</div>
	{/snippet}
</Story>
