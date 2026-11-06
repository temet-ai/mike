<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { fn, expect, userEvent, within, waitFor } from 'storybook/test';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import * as Sheet from './index';

	const { Story } = defineMeta({
		title: 'UI/Sheet/SheetContent',
		component: Sheet.Content,
		tags: ['autodocs'],
		argTypes: {
			showCloseButton: {
				control: 'boolean',
				description: 'Show or hide the built-in ghost-icon close button (default: true)'
			},
			class: {
				control: 'text',
				description: 'Additional Tailwind class merged via cn() alongside the base panel classes'
			}
		}
	});

	const onOpenChangeSpy = fn();

	let boundRef = $state<HTMLElement | null>(null);
</script>

<Story name="DefaultOpen" tags={['design-exploration']}>
	{#snippet template()}
		<div class="relative h-[500px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content aria-describedby={undefined}>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title>Panel title</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm">
							This is the default open state. The panel slides in from the right, occupies a fixed
							420px width, and fills the full viewport height. The close button (×) is rendered at
							the top-right corner via the built-in showCloseButton prop.
						</p>
						<p class="text-[var(--text-2)] text-xs">
							Background: <code class="font-mono">bg-[var(--surface-card)]</code> · Border:
							<code class="font-mono">border-l border-[var(--border-default)]</code>
						</p>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story name="WithShowCloseButton" tags={['design-exploration']}>
	{#snippet template()}
		<div class="relative h-[400px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content aria-describedby={undefined} showCloseButton={true}>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title>With close button</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm">
							The built-in close button (×) is rendered via showCloseButton=true. It is a ghost-icon
							Button at the top-right, inside a
							<code class="font-mono">DialogPrimitive.Close</code> wrapper.
						</p>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story name="WithoutCloseButton" tags={['design-exploration']}>
	{#snippet template()}
		<div class="relative h-[400px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content aria-describedby={undefined} showCloseButton={false}>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title>No built-in close button</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm">
							showCloseButton=false: the top-right × is not rendered. The caller is responsible for
							providing a close affordance.
						</p>
						<Sheet.Close>
							{#snippet child({ props })}
								<button
									class="mt-2 rounded-md border border-[var(--border-default)] bg-[var(--surface-2)] px-3 py-1.5 text-[var(--text-1)] text-sm hover:bg-[var(--muted)]"
									{...props}
								>
									Dismiss
								</button>
							{/snippet}
						</Sheet.Close>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story name="WithCustomClass" tags={['design-exploration']}>
	{#snippet template()}
		<div class="relative h-[400px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content
					aria-describedby={undefined}
					class="ring-4 ring-inset ring-[var(--status-info-fg)]/40"
				>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title>Custom class merged</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm">
							The ring-4 ring-inset ring-[var(--status-info-fg)]/40 class is merged via cn()
							alongside the fixed base classes (border-l, bg-[var(--surface-card)], shadow, flex
							flex-col, etc). The base appearance is preserved; the ring is added.
						</p>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story name="WithDescription" tags={['design-exploration']}>
	{#snippet template()}
		<div class="relative h-[450px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content>
					<div class="flex flex-col gap-4 p-5">
						<div class="space-y-1">
							<Sheet.Title>Request details</Sheet.Title>
							<Sheet.Description>
								Full request body, headers, and response for request
								<code class="font-mono text-[var(--text-1)]">req_01JT4X…</code>
							</Sheet.Description>
						</div>
						<div
							class="rounded-md border border-[var(--border-default)] bg-[var(--surface-2)] p-3 font-mono text-[var(--text-1)] text-xs"
						>
							<p class="text-[var(--text-2)]">// request body preview</p>
							<p class="mt-1">{`{ "model": "claude-opus-4-5", "max_tokens": 8192 }`}</p>
						</div>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="WithLongContent"
	tags={['interactive']}
	play={async () => {
		const sheet = await within(document.body).findByRole('dialog', { name: 'Long content panel' });
		await expect(getComputedStyle(sheet).getPropertyValue('--aug-border-all').trim()).toBe('1px');
		await expect(getComputedStyle(sheet).animationName).toBe('funk-sheet-in');
		const body = sheet.querySelector<HTMLElement>('[data-slot="sheet-body"]');
		if (!body) throw new Error('The sheet must expose its keyboard-scrollable body.');
		await waitFor(() => {
			expect(sheet.getBoundingClientRect().bottom).toBeLessThanOrEqual(window.innerHeight);
			expect(body.scrollHeight).toBeGreaterThan(body.clientHeight);
		});
		body.focus();
		await expect(body).toHaveFocus();
		body.scrollTop = body.scrollHeight;
		await expect(body.scrollTop).toBeGreaterThan(0);
		const finalItem = within(body).getByText(
			'Item 20 — the panel scrolls internally while the outer viewport stays fixed.'
		);
		await expect(finalItem.getBoundingClientRect().bottom).toBeLessThanOrEqual(window.innerHeight);
	}}
>
	{#snippet template()}
		<div class="relative h-[500px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content
					aria-describedby={undefined}
					aria-label="Long content panel"
					class="flex flex-col"
				>
					<div class="flex flex-col gap-1 border-b border-[var(--border-subtle)] px-5 py-4">
						<Sheet.Title>Long content panel</Sheet.Title>
					</div>
					<div class="flex-1 overflow-y-auto px-5 py-4 space-y-3">
						{#each Array.from({ length: 20 }, (_, i) => i + 1) as n (n)}
							<div
								class="rounded-md border border-[var(--border-subtle)] bg-[var(--surface-2)] px-3 py-2 text-[var(--text-1)] text-sm"
							>
								<span class="text-[var(--text-3)] font-mono text-xs mr-2">#{n}</span>
								Item {n} — the panel scrolls internally while the outer viewport stays fixed.
							</div>
						{/each}
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story name="WithPortalProps" tags={['design-exploration']}>
	{#snippet template()}
		<div class="relative h-[400px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content aria-describedby={undefined} portalProps={{ disabled: true }}>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title>Portal disabled</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm">
							<code>portalProps={'{ disabled: true }'}</code> keeps the SheetContent in the document tree
							rather than portaling to document.body. This exercises the portalProps forwarding path on
							SheetPortal.
						</p>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="VerifyRendering"
	tags={['interactive']}
	play={async () => {
		const content = document.body.querySelector('[data-slot="sheet-content"]');
		await expect(
			content,
			'data-slot="sheet-content" element should be present in document.body'
		).toBeTruthy();

		await expect(
			content?.getAttribute('data-state'),
			'data-state should be "open" when Sheet.Root is open'
		).toBe('open');
	}}
>
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content aria-describedby={undefined}>
					<div class="p-5">
						<Sheet.Title>Verify rendering</Sheet.Title>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="VerifyCloseButton"
	tags={['interactive']}
	play={async () => {
		const body = within(document.body);
		const closeBtn = body.getByRole('button', { name: /close/i });
		await expect(
			closeBtn,
			'close button (aria-label="Close") should be present in DOM'
		).toBeTruthy();

		const sheetClose = document.body.querySelector('[data-slot="sheet-close"]');
		await expect(
			sheetClose,
			'data-slot="sheet-close" wrapper should be present alongside the close button'
		).toBeTruthy();
	}}
>
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content aria-describedby={undefined}>
					<div class="p-5">
						<Sheet.Title>Verify close button</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm mt-2">
							The built-in × button should be present with aria-label="Close".
						</p>
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
		const content = document.body.querySelector('[data-slot="sheet-content"]');
		await expect(content, 'sheet-content should be in the DOM initially').toBeTruthy();

		await expect(
			content?.getAttribute('data-state'),
			'data-state should be "open" before click'
		).toBe('open');

		onOpenChangeSpy.mockClear();

		const body = within(document.body);
		const closeBtn = body.getByRole('button', { name: /close/i });
		await expect(closeBtn, 'close button should be present').toBeTruthy();

		await userEvent.click(closeBtn);

		await expect(onOpenChangeSpy).toHaveBeenCalledWith(false);
	}}
>
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open onOpenChange={onOpenChangeSpy}>
				<Sheet.Content aria-describedby={undefined}>
					<div class="p-5">
						<Sheet.Title>Click × to close</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm mt-2">
							Click the close button to dismiss the panel.
						</p>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="ClosesOnEscape"
	tags={['interactive']}
	play={async () => {
		const content = document.body.querySelector('[data-slot="sheet-content"]');
		await expect(content, 'sheet-content should be in DOM before Escape').toBeTruthy();

		await expect(
			content?.getAttribute('data-state'),
			'data-state should be "open" before Escape key'
		).toBe('open');

		onOpenChangeSpy.mockClear();

		await userEvent.keyboard('{Escape}');

		await waitFor(() => expect(onOpenChangeSpy).toHaveBeenCalledWith(false), { timeout: 2000 });
	}}
>
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open onOpenChange={onOpenChangeSpy}>
				<Sheet.Content aria-describedby={undefined}>
					<div class="p-5">
						<Sheet.Title>Press Escape to close</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm mt-2">
							Escape key dismisses the panel via the bits-ui EscapeLayer.
						</p>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="VerifyNoCloseButton"
	tags={['interactive']}
	play={async () => {
		const sheetClose = document.body.querySelector('[data-slot="sheet-close"]');
		await expect(
			sheetClose,
			'data-slot="sheet-close" should NOT be in DOM when showCloseButton=false'
		).toBeNull();

		const content = document.body.querySelector('[data-slot="sheet-content"]');
		await expect(content, 'sheet-content should still be present').toBeTruthy();

		const closeBtnInPanel = content?.querySelector('button[aria-label="Close"]');
		await expect(
			closeBtnInPanel,
			'no button with aria-label="Close" should be inside sheet-content when showCloseButton=false'
		).toBeNull();
	}}
>
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content aria-describedby={undefined} showCloseButton={false}>
					<div class="p-5">
						<Sheet.Title>No close button</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm mt-2">
							showCloseButton=false — the built-in × is not rendered.
						</p>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="VerifyClassMerge"
	tags={['interactive']}
	play={async () => {
		const content = document.body.querySelector<HTMLElement>('[data-slot="sheet-content"]');
		await expect(content, 'sheet-content should be in document.body').toBeTruthy();

		await expect(
			content?.classList.contains('border-2'),
			'custom class "border-2" should be present after cn() merge'
		).toBe(true);
	}}
>
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content
					aria-describedby={undefined}
					class="border-2 border-[var(--interactive-active-fg)]"
				>
					<div class="p-5">
						<Sheet.Title>Class merge verification</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm mt-2">
							class="border-2 border-[var(--interactive-active-fg)]" merges via cn() alongside the
							base classes (border-l, bg-[var(--surface-card)], etc.).
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
		await expect(
			boundRef,
			'boundRef should be non-null after Sheet.Content mounts (bind:ref populated)'
		).not.toBeNull();

		await expect(
			boundRef instanceof HTMLElement,
			'boundRef should be an HTMLElement (matches $bindable<HTMLElement | null>)'
		).toBe(true);

		await expect(
			boundRef?.getAttribute('data-slot'),
			'bound ref element should carry data-slot="sheet-content"'
		).toBe('sheet-content');

		await expect(
			boundRef?.getAttribute('data-state'),
			'bound ref element should carry data-state="open" when sheet is open'
		).toBe('open');
	}}
>
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content bind:ref={boundRef} aria-describedby={undefined}>
					<div class="p-5">
						<Sheet.Title>Ref binding test</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm mt-2">
							bind:ref={boundRef} populates the module-level $state variable with the underlying DialogPrimitive.Content
							HTMLElement portaled to document.body. The play function asserts on boundRef directly, not
							via a fresh DOM query.
						</p>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="RestPropsForwarding"
	tags={['interactive']}
	play={async () => {
		const content = document.body.querySelector<HTMLElement>('[data-slot="sheet-content"]');
		await expect(content, 'sheet-content should be present in document.body').not.toBeNull();

		await expect(
			content?.getAttribute('id'),
			'id="test-sheet" restProp should be forwarded to the portaled element'
		).toBe('test-sheet');

		await expect(
			content?.getAttribute('data-testid'),
			'data-testid="my-sheet" restProp should be forwarded to the portaled element'
		).toBe('my-sheet');
	}}
>
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content aria-describedby={undefined} id="test-sheet" data-testid="my-sheet">
					<div class="p-5">
						<Sheet.Title>Rest props forwarding</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm mt-2">
							id="test-sheet" and data-testid="my-sheet" are forwarded via ...restProps to
							DialogPrimitive.Content. The play function verifies both attributes are present on the
							portaled element.
						</p>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="OpenStateSnapshot"
	tags={['visual']}
	play={async () => {
		await captureStorySnapshot(document.body as HTMLElement, 'sheet-content/OpenState');
	}}
>
	{#snippet template()}
		<div class="relative h-[500px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content aria-describedby={undefined}>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title>Help &amp; references</Sheet.Title>
						<p class="text-[var(--text-1)] text-sm">
							This panel slides in from the right. It occupies a fixed 420px maximum width and fills
							the full viewport height, anchored to the trailing edge of the screen.
						</p>
						<p class="text-[var(--text-2)] text-xs">
							Background: <code class="font-mono">bg-[var(--surface-card)]</code>
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
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'sheet-content/AllThemes');
	}}
>
	{#snippet template()}
		<div class="flex flex-col gap-3 p-4 bg-[var(--surface-bg)]">
			<p class="text-[var(--text-2)] text-xs font-mono mb-1">
				SheetContent surface — bg-[var(--surface-card)] border-l border-[var(--border-default)]
				across 5 themes
			</p>
			{#each ['forge', 'glacier', 'void', 'phosphor', 'crimson'] as theme (theme)}
				<div
					data-theme={theme}
					class="flex items-stretch rounded-md overflow-hidden border border-[var(--border-subtle)]"
				>
					<div class="bg-[var(--surface-bg)] flex items-center px-3 py-3 min-w-[80px]">
						<span class="text-[var(--text-3)] text-xs font-mono">{theme}</span>
					</div>
					<Sheet.Root open>
						<Sheet.Content
							aria-describedby={undefined}
							portalProps={{ disabled: true }}
							class="static inset-auto max-w-none flex-1 shadow-none"
						>
							<div class="px-4 py-3">
								<Sheet.Title>Panel title</Sheet.Title>
								<p class="text-[var(--text-2)] text-xs mt-1">
									Surface and border tokens per theme.
								</p>
							</div>
						</Sheet.Content>
					</Sheet.Root>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>
