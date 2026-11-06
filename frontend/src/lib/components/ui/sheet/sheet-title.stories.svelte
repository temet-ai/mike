<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, fn } from 'storybook/test';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import * as Sheet from './index';

	const { Story } = defineMeta({
		title: 'UI/Sheet/SheetTitle',
		component: Sheet.Title,
		tags: ['autodocs']
	});
</script>

<script lang="ts">
	let refEl: HTMLElement | null = $state(null);
</script>

<Story name="Default">
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title>Request details</Sheet.Title>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story name="WithChildSnippet">
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title>
							Request <code class="font-mono text-sm">req_01JT4X…</code>
						</Sheet.Title>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story name="WithCustomClass">
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title class="uppercase tracking-wide">Custom class merged title</Sheet.Title>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story name="WithAriaAttributes">
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title id="aria-test-title" data-testid="sheet-title-aria">
							Accessible sheet title
						</Sheet.Title>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story name="LongTextOverflow">
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title>
							Session inspection — captured API traffic for the current request including headers,
							body, and streaming response
						</Sheet.Title>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="EmptyContent"
	parameters={{
		a11y: {
			config: {
				rules: [{ id: 'empty-heading', enabled: false }]
			}
		}
	}}
>
	{#snippet template()}
		<div class="relative h-[250px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content aria-label="Sheet with empty title" aria-describedby={undefined}>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title />
						<p class="text-[var(--text-1)] text-sm">
							The title above is empty — the component renders a zero-height heading with
							data-slot="sheet-title" but no visible content.
						</p>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story name="PairedWithDescription">
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content>
					<div class="flex flex-col gap-4 p-5">
						<div class="space-y-1">
							<Sheet.Title>Request details</Sheet.Title>
							<Sheet.Description>
								Full request body, headers, and response for this captured API call.
							</Sheet.Description>
						</div>
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
		await captureStorySnapshot(canvasElement, 'sheet-title/AllThemes');
	}}
>
	{#snippet template()}
		<div class="flex flex-col gap-3 p-4 bg-[var(--surface-bg)]">
			<p class="text-[var(--text-2)] text-xs font-mono mb-1">
				SheetTitle — text-[var(--text-strong)] text-base leading-none font-semibold across all 5
				themes
			</p>
			{#each ['forge', 'glacier', 'void', 'phosphor', 'crimson'] as theme}
				<div
					data-theme={theme}
					class="flex flex-col gap-1 rounded-md border border-[var(--border-subtle)] bg-[var(--surface-card)] px-4 py-3"
				>
					<span class="text-[var(--text-3)] text-xs font-mono mb-1">{theme}</span>
					<h2
						data-slot="sheet-title"
						class="text-[var(--text-strong)] text-base leading-none font-semibold"
					>
						Request details
					</h2>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>

<Story
	name="DefaultSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'sheet-title/Default');
	}}
>
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content>
					<div class="flex flex-col gap-4 p-5">
						<div class="space-y-1">
							<Sheet.Title>Request details</Sheet.Title>
							<Sheet.Description>
								Full request body, headers, and response for this captured API call.
							</Sheet.Description>
						</div>
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
		const anchor = document.body.querySelector('[data-story="verify-rendering"]');
		await expect(
			anchor,
			'data-story="verify-rendering" anchor should be present in the portal'
		).not.toBeNull();

		const title = anchor?.querySelector('[data-slot="sheet-title"]');

		await expect(
			title?.textContent?.trim(),
			'title textContent should match the supplied text'
		).toBe('Request details');

		await expect(
			title?.getAttribute('data-slot'),
			'title element should carry data-slot="sheet-title"'
		).toBe('sheet-title');
	}}
>
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content>
					<div data-story="verify-rendering" class="flex flex-col gap-4 p-5">
						<Sheet.Title>Request details</Sheet.Title>
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
		const anchor = document.body.querySelector('[data-story="verify-class-merge"]');
		await expect(
			anchor,
			'data-story="verify-class-merge" anchor should be present in the portal'
		).not.toBeNull();

		const title = anchor?.querySelector('[data-slot="sheet-title"]');
		await expect(title, 'sheet-title should be present in the DOM').not.toBeNull();

		await expect(
			title?.classList.contains('uppercase'),
			'custom class="uppercase" should be present on the title element after cn() merge'
		).toBe(true);

		await expect(
			title?.classList.contains('tracking-wide'),
			'custom class="tracking-wide" should be present on the title element after cn() merge'
		).toBe(true);

		await expect(
			title?.classList.contains('font-semibold'),
			'base font-semibold class should remain on the title element after cn() merge'
		).toBe(true);
	}}
>
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content>
					<div data-story="verify-class-merge" class="flex flex-col gap-4 p-5">
						<Sheet.Title class="uppercase tracking-wide">Custom class merge test</Sheet.Title>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="VerifyAriaForwarding"
	tags={['interactive']}
	play={async () => {
		const anchor = document.body.querySelector('[data-story="verify-aria-forwarding"]');
		await expect(
			anchor,
			'data-story="verify-aria-forwarding" anchor should be present in the portal'
		).not.toBeNull();

		const title = anchor?.querySelector('[data-slot="sheet-title"]');
		await expect(title, 'sheet-title should be present in the DOM').not.toBeNull();

		await expect(
			title?.getAttribute('id'),
			'id="aria-test-title" should be forwarded to the title element via restProps'
		).toBe('aria-test-title');

		await expect(
			title?.getAttribute('data-testid'),
			'data-testid="sheet-title-aria" should be forwarded through restProps'
		).toBe('sheet-title-aria');
	}}
>
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content>
					<div data-story="verify-aria-forwarding" class="flex flex-col gap-4 p-5">
						<Sheet.Title id="aria-test-title" data-testid="sheet-title-aria">
							Aria forwarding test
						</Sheet.Title>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="VerifyTextStyling"
	tags={['interactive']}
	play={async () => {
		const anchor = document.body.querySelector('[data-story="verify-text-styling"]');
		await expect(
			anchor,
			'data-story="verify-text-styling" anchor should be present in the portal'
		).not.toBeNull();

		const title = anchor?.querySelector<HTMLElement>('[data-slot="sheet-title"]');
		await expect(title, 'sheet-title should be present in the DOM').not.toBeNull();

		if (!title) return;

		const computedStyle = window.getComputedStyle(title);

		await expect(
			computedStyle.fontSize,
			'text-base should resolve to 16px computed font-size'
		).toBe('16px');

		await expect(computedStyle.fontWeight, 'font-semibold should resolve to font-weight 600').toBe(
			'600'
		);
	}}
>
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content>
					<div data-story="verify-text-styling" class="flex flex-col gap-4 p-5">
						<Sheet.Title>Styling verification title</Sheet.Title>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="RefBinding"
	tags={['interactive']}
	args={{ onclick: fn() }}
	play={async ({ canvasElement, args }) => {
		const anchor = document.body.querySelector('[data-story="ref-binding"]');
		await expect(
			anchor,
			'data-story="ref-binding" anchor should be present in the portal'
		).not.toBeNull();

		const title = anchor?.querySelector<HTMLElement>('[data-slot="sheet-title"]');
		await expect(title, 'sheet-title element should exist in the DOM (ref target)').not.toBeNull();

		await expect(
			title?.tagName,
			'title ref should point to a DIV element (bits-ui DialogPrimitive.Title renders as div, not h2)'
		).toBe('DIV');

		await expect(
			title?.getAttribute('data-slot'),
			'ref element should carry data-slot="sheet-title"'
		).toBe('sheet-title');

		const refSetMarker = anchor?.querySelector('[data-ref-set="true"]');
		await expect(
			refSetMarker,
			'data-ref-set="true" should be present — confirms bind:ref populated refEl'
		).not.toBeNull();

		await userEvent.click(title!);
		await expect(args.onclick).toHaveBeenCalledWith(expect.objectContaining({ type: 'click' }));
	}}
>
	{#snippet template(args)}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content>
					<div data-story="ref-binding" class="flex flex-col gap-4 p-5">
						<Sheet.Title bind:ref={refEl} onclick={args.onclick}>Ref binding test</Sheet.Title>
						{#if refEl !== null}
							<span data-ref-set="true" class="sr-only" aria-hidden="true"></span>
						{/if}
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>
