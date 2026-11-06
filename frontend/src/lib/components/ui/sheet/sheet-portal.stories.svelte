<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import * as Sheet from './index';
	import SheetPortal from './sheet-portal.svelte';
	import SheetOverlay from './sheet-overlay.svelte';

	const { Story } = defineMeta({
		title: 'UI/Sheet/SheetPortal',
		component: SheetPortal,
		tags: ['autodocs'],
		argTypes: {
			to: {
				control: 'text',
				description:
					'Where to teleport the content. Accepts a CSS selector string or an Element. Default: document.body.'
			},
			disabled: {
				control: 'boolean',
				description: 'Disable portalling — render children inline at their source position.'
			}
		}
	});
</script>

<Story
	name="DefaultPortal"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const sentinelInCanvas = canvasElement.querySelector('[data-portal-sentinel="default"]');
		await expect(
			sentinelInCanvas,
			'sentinel must NOT be inside canvasElement — it was portalled out'
		).toBeNull();

		const sentinelInDoc = document.querySelector('[data-portal-sentinel="default"]');

		await expect(
			sentinelInDoc instanceof HTMLElement,
			'sentinel must be an HTMLElement in the document'
		).toBe(true);
		await expect(
			(sentinelInDoc as HTMLElement).dataset.portalSentinel,
			'sentinel data-portal-sentinel attribute must equal "default"'
		).toBe('default');

		await expect(
			canvasElement.contains(sentinelInDoc),
			'sentinel must not be a descendant of canvasElement'
		).toBe(false);
	}}
>
	{#snippet template()}
		<div
			class="relative h-[200px] bg-[var(--surface-bg)] rounded-md border border-[var(--border-subtle)] flex items-center justify-center"
		>
			<Sheet.Root open>
				<SheetPortal>
					<div data-portal-sentinel="default" aria-hidden="true" class="sr-only"></div>
					<SheetOverlay />
				</SheetPortal>
			</Sheet.Root>
			<span
				class="relative z-[52] text-[var(--text-strong)] text-xs font-mono px-2 py-1 bg-[var(--surface-2)] rounded pointer-events-none"
			>
				portal active → document.body
			</span>
		</div>
	{/snippet}
</Story>

<Story
	name="Disabled"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const sentinelInCanvas = canvasElement.querySelector('[data-portal-sentinel="disabled"]');

		await expect(
			sentinelInCanvas instanceof HTMLElement,
			'sentinel must be an HTMLElement inside canvasElement when portal is disabled'
		).toBe(true);
		await expect(
			(sentinelInCanvas as HTMLElement).dataset.portalSentinel,
			'sentinel data-portal-sentinel attribute must equal "disabled"'
		).toBe('disabled');

		await expect(
			canvasElement.contains(sentinelInCanvas),
			'sentinel must be a descendant of canvasElement when disabled'
		).toBe(true);
	}}
>
	{#snippet template()}
		<div
			class="relative h-[200px] bg-[var(--surface-bg)] rounded-md border border-[var(--border-subtle)] flex items-center justify-center"
		>
			<Sheet.Root open>
				<SheetPortal disabled>
					<div data-portal-sentinel="disabled" aria-hidden="true" class="sr-only"></div>
					<SheetOverlay />
				</SheetPortal>
			</Sheet.Root>
			<span
				class="relative z-[52] text-[var(--text-strong)] text-xs font-mono px-2 py-1 bg-[var(--surface-2)] rounded pointer-events-none"
			>
				disabled — inline render, no teleport
			</span>
		</div>
	{/snippet}
</Story>

<Story
	name="CustomTarget"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const target = canvasElement.querySelector('#portal-target');
		await expect(
			target instanceof HTMLElement,
			'#portal-target container must be an HTMLElement in canvasElement'
		).toBe(true);

		const sentinelInTarget = (target as HTMLElement).querySelector(
			'[data-portal-sentinel="custom"]'
		);
		await expect(
			sentinelInTarget instanceof HTMLElement,
			'sentinel must be an HTMLElement inside #portal-target (custom portal destination)'
		).toBe(true);
		await expect(
			(sentinelInTarget as HTMLElement).dataset.portalSentinel,
			'sentinel data-portal-sentinel attribute must equal "custom"'
		).toBe('custom');

		const originWrapper = canvasElement.querySelector('[data-portal-origin]');
		await expect(
			originWrapper instanceof HTMLElement,
			'[data-portal-origin] marker must be present in the template — if absent this assertion would pass vacuously'
		).toBe(true);
		await expect(
			(originWrapper as HTMLElement).contains(sentinelInTarget),
			'sentinel must NOT be inside the Sheet.Root origin wrapper — it was teleported to #portal-target'
		).toBe(false);
	}}
>
	{#snippet template()}
		<div
			class="relative h-[240px] bg-[var(--surface-bg)] rounded-md border border-[var(--border-subtle)] p-4 flex flex-col gap-4"
		>
			<div
				id="portal-target"
				class="rounded border border-dashed border-[var(--status-info-fg)] p-3 min-h-[80px] relative"
			>
				<span
					class="absolute top-1 left-2 text-[var(--status-info-fg)] text-[10px] font-mono pointer-events-none"
				>
					#portal-target (receives content)
				</span>
			</div>

			<div data-portal-origin>
				<Sheet.Root open>
					<SheetPortal to="#portal-target">
						<div data-portal-sentinel="custom" aria-hidden="true" class="sr-only"></div>
						<span class="text-[var(--text-2)] text-xs font-mono">
							teleported here via to="#portal-target"
						</span>
					</SheetPortal>
				</Sheet.Root>
			</div>

			<span class="text-[var(--text-3)] text-xs font-mono"> source: Sheet.Root below target </span>
		</div>
	{/snippet}
</Story>

<Story
	name="WithContent"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'sheet-portal/WithContent');
	}}
>
	{#snippet template()}
		<div
			class="relative h-[400px] overflow-hidden bg-[var(--surface-bg)] rounded-md border border-[var(--border-subtle)]"
		>
			<Sheet.Root open>
				<SheetPortal>
					<SheetOverlay />
					<Sheet.Content aria-describedby={undefined}>
						<div class="flex flex-col gap-4 p-5">
							<Sheet.Title>Portalled sheet content</Sheet.Title>
							<p class="text-[var(--text-1)] text-sm">
								SheetPortal teleports SheetOverlay + SheetContent to
								<code class="font-mono text-[var(--text-strong)]">document.body</code>, so the fixed
								positioning works regardless of any parent
								<code class="font-mono text-[var(--text-strong)]">overflow:hidden</code>
								or <code class="font-mono text-[var(--text-strong)]">transform</code> context.
							</p>
							<p class="text-[var(--text-2)] text-xs">
								Production usage: SheetContent always wraps both SheetOverlay and Dialog.Content
								inside a single SheetPortal instance.
							</p>
						</div>
					</Sheet.Content>
				</SheetPortal>
			</Sheet.Root>
		</div>
	{/snippet}
</Story>

<Story
	name="DisabledSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'sheet-portal/Disabled');
	}}
>
	{#snippet template()}
		<div
			class="relative h-[200px] overflow-hidden bg-[var(--surface-card)] rounded-md border border-[var(--border-subtle)]"
		>
			<Sheet.Root open>
				<SheetPortal disabled>
					<SheetOverlay />
				</SheetPortal>
			</Sheet.Root>
			<div class="absolute inset-0 flex items-center justify-center pointer-events-none">
				<span
					class="text-[var(--text-strong)] text-xs font-mono px-2 py-1 bg-[var(--surface-2)] rounded z-[52]"
				>
					disabled — overlay clipped by overflow:hidden
				</span>
			</div>
		</div>
	{/snippet}
</Story>

<Story
	name="DefaultPortalSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'sheet-portal/DefaultPortal');
	}}
>
	{#snippet template()}
		<div
			class="relative h-[200px] overflow-hidden bg-[var(--surface-card)] rounded-md border border-[var(--border-subtle)]"
		>
			<Sheet.Root open>
				<SheetPortal>
					<SheetOverlay />
				</SheetPortal>
			</Sheet.Root>
			<div class="absolute inset-0 flex items-center justify-center pointer-events-none z-[52]">
				<span
					class="text-[var(--text-strong)] text-xs font-mono px-2 py-1 bg-[var(--surface-2)] rounded"
				>
					default portal → document.body
				</span>
			</div>
		</div>
	{/snippet}
</Story>

<Story
	name="AllThemesSnapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'sheet-portal/AllThemes');
	}}
>
	{#snippet template()}
		<div class="flex gap-4 flex-wrap p-2">
			{#each ['forge', 'glacier', 'void', 'phosphor', 'crimson'] as theme}
				<div
					data-theme={theme}
					class="relative h-[200px] w-[180px] overflow-hidden rounded-md border border-[var(--border-subtle)] bg-[var(--surface-bg)]"
				>
					<Sheet.Root open>
						<SheetPortal disabled>
							<SheetOverlay />
							<Sheet.Content aria-describedby={undefined}>
								<div class="p-3">
									<Sheet.Title class="text-xs">{theme}</Sheet.Title>
								</div>
							</Sheet.Content>
						</SheetPortal>
					</Sheet.Root>
					<div class="absolute bottom-1 inset-x-0 flex justify-center pointer-events-none z-[52]">
						<span
							class="text-[var(--text-strong)] text-[10px] font-mono px-1.5 py-0.5 bg-[var(--surface-2)] rounded"
						>
							{theme}
						</span>
					</div>
				</div>
			{/each}
		</div>
	{/snippet}
</Story>
