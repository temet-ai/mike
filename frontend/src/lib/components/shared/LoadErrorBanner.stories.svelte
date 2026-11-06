<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent } from 'storybook/test';
	import LoadErrorBanner from './LoadErrorBanner.svelte';

	interface LoadErrorProps {
		title?: string;
		detail?: string;
		hint?: string;
		onRetry?: () => void;
		backHref?: string;
		backLabel?: string;
		docsHref?: string;
		docsLabel?: string;
		message?: string;
	}

	function buildLoadErrorProps(overrides: Partial<LoadErrorProps> = {}): LoadErrorProps {
		return {
			title: "Couldn't load this session",
			detail: 'fetch failed: 404 Not Found',
			...overrides
		};
	}

	function slotText(canvasElement: HTMLElement, slot: string): string {
		return (canvasElement.querySelector(`[data-slot="${slot}"]`)?.textContent ?? '')
			.replace(/\s+/g, ' ')
			.trim();
	}

	const { Story } = defineMeta({
		title: 'Shared/LoadErrorBanner',
		component: LoadErrorBanner,
		tags: ['autodocs']
	});
</script>

<Story
	name="Typical"
	tags={['interactive']}
	args={buildLoadErrorProps()}
	play={async ({ canvasElement }) => {
		expect(slotText(canvasElement, 'load-error-title')).toBe("Couldn't load this session");
		expect(slotText(canvasElement, 'load-error-detail')).toBe('fetch failed: 404 Not Found');
		expect(canvasElement.querySelector('[data-slot="load-error-actions"]')).toBeNull();
	}}
>
	{#snippet template(args)}
		<div class="p-4" style="max-width: 480px;">
			<LoadErrorBanner {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="WithHint"
	tags={['interactive']}
	args={buildLoadErrorProps({
		hint: 'The session may have been evicted by the TTL policy. Try opening a recent session instead.'
	})}
	play={async ({ canvasElement }) => {
		expect(slotText(canvasElement, 'load-error-title')).toBe("Couldn't load this session");
		expect(slotText(canvasElement, 'load-error-hint')).toContain(
			'The session may have been evicted by the TTL policy.'
		);
	}}
>
	{#snippet template(args)}
		<div class="p-4" style="max-width: 480px;">
			<LoadErrorBanner {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="WithRetry"
	tags={['interactive']}
	args={buildLoadErrorProps({
		onRetry: fn()
	})}
	play={async ({ canvasElement, args }) => {
		const retry = canvasElement.querySelector(
			'[data-slot="load-error-retry"]'
		) as HTMLButtonElement | null;
		expect(canvasElement.querySelector('[data-slot="load-error-retry"]')).not.toBeNull();
		expect(canvasElement.querySelector('[data-slot="load-error-back"]')).toBeNull();
		expect(canvasElement.querySelector('[data-slot="load-error-docs"]')).toBeNull();
		await userEvent.click(retry!);
		expect(args.onRetry).toHaveBeenCalledTimes(1);
	}}
>
	{#snippet template(args)}
		<div class="p-4" style="max-width: 480px;">
			<LoadErrorBanner {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="FullActions"
	tags={['interactive']}
	args={buildLoadErrorProps({
		hint: 'The session may have been evicted by the TTL policy.',
		onRetry: fn(),
		backHref: '/sessions',
		backLabel: 'Back to sessions',
		docsHref: '/help/glossary#session',
		docsLabel: 'What is a session?'
	})}
	play={async ({ canvasElement, args }) => {
		const retry = canvasElement.querySelector(
			'[data-slot="load-error-retry"]'
		) as HTMLButtonElement | null;
		const back = canvasElement.querySelector(
			'[data-slot="load-error-back"]'
		) as HTMLAnchorElement | null;
		const docs = canvasElement.querySelector(
			'[data-slot="load-error-docs"]'
		) as HTMLAnchorElement | null;
		expect(retry).not.toBeNull();
		expect(back?.getAttribute('href')).toBe('/sessions');
		expect(back?.textContent).toContain('Back to sessions');
		expect(docs?.getAttribute('href')).toBe('/help/glossary#session');
		expect(docs?.getAttribute('target')).toBeNull();
		expect(docs?.getAttribute('rel')).toBeNull();
		await userEvent.click(retry!);
		expect(args.onRetry).toHaveBeenCalledTimes(1);
	}}
>
	{#snippet template(args)}
		<div class="p-4" style="max-width: 480px;">
			<LoadErrorBanner {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="LegacyShape"
	tags={['interactive']}
	args={{ message: 'fetch failed', onRetry: fn() }}
	play={async ({ canvasElement, args }) => {
		const retry = canvasElement.querySelector(
			'[data-slot="load-error-retry"]'
		) as HTMLButtonElement | null;
		expect(slotText(canvasElement, 'load-error-title')).toBe('Failed to load.');
		expect(slotText(canvasElement, 'load-error-detail')).toBe('fetch failed');
		expect(canvasElement.querySelector('[data-slot="load-error-retry"]')).not.toBeNull();
		expect(canvasElement.querySelector('[data-slot="load-error-back"]')).toBeNull();
		expect(canvasElement.querySelector('[data-slot="load-error-docs"]')).toBeNull();
		await userEvent.click(retry!);
		expect(args.onRetry).toHaveBeenCalledTimes(1);
	}}
>
	{#snippet template(args)}
		<div class="p-4" style="max-width: 480px;">
			<LoadErrorBanner {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="ExternalDocs"
	tags={['interactive']}
	args={buildLoadErrorProps({
		docsHref: 'https://docs.anthropic.com/',
		docsLabel: 'Open architecture docs'
	})}
	play={async ({ canvasElement }) => {
		const docs = canvasElement.querySelector(
			'[data-slot="load-error-docs"]'
		) as HTMLAnchorElement | null;
		expect(docs?.getAttribute('href')).toBe('https://docs.anthropic.com/');
		expect(docs?.getAttribute('target')).toBe('_blank');
		expect(docs?.getAttribute('rel')).toContain('noopener');
	}}
>
	{#snippet template(args)}
		<div class="p-4" style="max-width: 480px;">
			<LoadErrorBanner {...args} />
		</div>
	{/snippet}
</Story>
