<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, waitFor } from 'storybook/test';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import * as Sheet from './index';

	const { Story } = defineMeta({
		title: 'UI/Sheet/SheetDescription',
		component: Sheet.Description,
		tags: ['autodocs']
	});

	let refBindingTarget = $state<HTMLElement | null>(null);
</script>

<Story name="Default">
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

<Story name="WithChildSnippet">
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
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
						<div class="space-y-1">
							<Sheet.Title>Custom class sheet</Sheet.Title>
							<Sheet.Description class="italic">
								This description has an additional italic class merged via cn() alongside the base
								text-[var(--text-2)] text-xs styling.
							</Sheet.Description>
						</div>
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
						<div class="space-y-1">
							<Sheet.Title id="aria-test-title">Accessible sheet</Sheet.Title>
							<Sheet.Description id="aria-test-description">
								This description has an explicit id so external elements can reference it via
								aria-describedby for enhanced screen reader announcements.
							</Sheet.Description>
						</div>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story name="LongTextOverflow">
	{#snippet template()}
		<div class="relative h-[400px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content>
					<div class="flex flex-col gap-4 p-5">
						<div class="space-y-1">
							<Sheet.Title>Detailed inspection</Sheet.Title>
							<Sheet.Description>
								This is a longer description that spans multiple lines to verify that the text-xs
								(0.75rem) font size and the standard line-height render correctly within the sheet's
								420px maximum width. The text should wrap naturally without overflow, clipping, or
								horizontal scrolling. Each line should be readable at the secondary label color
								(--text-2) in all five project themes.
							</Sheet.Description>
						</div>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story name="EmptyContent">
	{#snippet template()}
		<div class="relative h-[250px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content>
					<div class="flex flex-col gap-4 p-5">
						<Sheet.Title>Sheet with empty description</Sheet.Title>
						<Sheet.Description />
						<p class="text-[var(--text-1)] text-sm">
							The description above is empty — the component renders a zero-height div with
							data-slot="sheet-description" but no visible content.
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
		await captureStorySnapshot(canvasElement, 'sheet-description/AllThemes');
	}}
>
	{#snippet template()}
		<div class="flex flex-col gap-3 p-4 bg-[var(--surface-bg)]">
			<p class="text-[var(--text-2)] text-xs font-mono mb-1">
				SheetDescription — text-[var(--text-2)] text-xs across all 5 themes
			</p>
			{#each ['forge', 'glacier', 'void', 'phosphor', 'crimson'] as theme (theme)}
				<div
					data-theme={theme}
					class="relative h-[120px] overflow-hidden rounded-md border border-[var(--border-subtle)] bg-[var(--surface-card)]"
				>
					<span class="absolute top-2 left-3 text-[var(--text-3)] text-xs font-mono">{theme}</span>
					<Sheet.Root open>
						<Sheet.Content>
							<div class="flex flex-col gap-2 p-5 pt-8">
								<Sheet.Title>{theme} theme</Sheet.Title>
								<Sheet.Description>
									Full request body, headers, and response for request
									<code class="font-mono text-[var(--text-1)]">req_01JT4X…</code>
								</Sheet.Description>
							</div>
						</Sheet.Content>
					</Sheet.Root>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>

<Story
	name="DefaultSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'sheet-description/Default');
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
								Full request body, headers, and response for request
								<code class="font-mono text-[var(--text-1)]">req_01JT4X…</code>
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
		const description = document.body.querySelector('[data-slot="sheet-description"]');
		await expect(
			description,
			'sheet-description div should be present with data-slot="sheet-description"'
		).toBeTruthy();

		await expect(
			description?.textContent?.trim(),
			'description textContent should include the supplied text'
		).toContain('Full request body, headers, and response');
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
								Full request body, headers, and response for this API call.
							</Sheet.Description>
						</div>
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
		const description = document.body.querySelector('[data-slot="sheet-description"]');
		await expect(description, 'sheet-description should be present in the DOM').toBeTruthy();

		await expect(
			description?.classList.contains('italic'),
			'custom class="italic" should be present on the description element after cn() merge'
		).toBe(true);

		await expect(
			description?.classList.contains('text-xs'),
			'base class text-xs should still be present after cn() merge'
		).toBe(true);

		await expect(
			description?.classList.contains('text-[var(--text-2)]'),
			'base class text-[var(--text-2)] should still be present after cn() merge'
		).toBe(true);
	}}
>
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content>
					<div class="flex flex-col gap-4 p-5">
						<div class="space-y-1">
							<Sheet.Title>Custom class merge test</Sheet.Title>
							<Sheet.Description class="italic">
								This description merges class="italic" with the base text-[var(--text-2)] text-xs.
							</Sheet.Description>
						</div>
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
		const description = document.body.querySelector('[data-slot="sheet-description"]');
		await expect(description, 'sheet-description should be present in the DOM').toBeTruthy();

		await expect(
			description?.getAttribute('id'),
			'id="desc-test" should be forwarded to the description element via restProps'
		).toBe('desc-test');

		await expect(
			description?.getAttribute('data-testid'),
			'data-testid should be forwarded through restProps'
		).toBe('my-desc');
	}}
>
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content>
					<div class="flex flex-col gap-4 p-5">
						<div class="space-y-1">
							<Sheet.Title id="title-test">Aria forwarding test</Sheet.Title>
							<Sheet.Description id="desc-test" data-testid="my-desc">
								This description has id="desc-test" forwarded via restProps so external elements can
								wire aria-describedby to it.
							</Sheet.Description>
						</div>
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
		const description = document.body.querySelector<HTMLElement>('[data-slot="sheet-description"]');
		await expect(description, 'sheet-description should be present in the DOM').toBeTruthy();

		if (!description) return;

		const computedStyle = window.getComputedStyle(description);
		await expect(
			computedStyle.fontSize,
			'text-xs should resolve to the active FUNK text-xs token'
		).toBe(computedStyle.getPropertyValue('--text-xs').trim());
	}}
>
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content>
					<div class="flex flex-col gap-4 p-5">
						<div class="space-y-1">
							<Sheet.Title>Styling verification</Sheet.Title>
							<Sheet.Description>
								This description verifies text-xs follows the active FUNK type scale via computed
								styles.
							</Sheet.Description>
						</div>
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
		await waitFor(() => expect(refBindingTarget).not.toBeNull(), { timeout: 2000 });

		await expect(
			refBindingTarget,
			'ref $bindable should be populated after sheet mounts (bind:ref={refBindingTarget} must write a non-null value)'
		).not.toBeNull();

		const target = refBindingTarget as unknown as HTMLElement;

		await expect(
			target.tagName,
			'ref should point to a DIV element (bits-ui DialogPrimitive.Description renders as div)'
		).toBe('DIV');

		await expect(
			target.getAttribute('data-slot'),
			'ref element should carry data-slot="sheet-description" — it must be the description div, not a portal wrapper'
		).toBe('sheet-description');

		const domDescription = document.body.querySelector<HTMLElement>(
			'[data-slot="sheet-description"]'
		);
		await expect(
			target === domDescription,
			'ref must point to the same element as document.body.querySelector(\'[data-slot="sheet-description"]\')'
		).toBe(true);
	}}
>
	{#snippet template()}
		<div class="relative h-[300px] overflow-hidden bg-[var(--surface-bg)]">
			<Sheet.Root open>
				<Sheet.Content>
					<div class="flex flex-col gap-4 p-5">
						<div class="space-y-1">
							<Sheet.Title>Ref binding test</Sheet.Title>
							<Sheet.Description bind:ref={refBindingTarget}>
								The ref prop ($bindable null) is bound via bind:ref to the underlying div. The play
								function asserts refBindingTarget holds an HTMLDivElement with
								data-slot="sheet-description".
							</Sheet.Description>
						</div>
					</div>
				</Sheet.Content>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>
