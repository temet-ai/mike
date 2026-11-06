<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor } from 'storybook/test';
	import Tooltip from './Tooltip.svelte';
	import { Copy, Pin, ExternalLink } from '@lucide/svelte';

	function tooltipContent(): HTMLElement | null {
		return document.body.querySelector('[data-slot="tooltip-text"]') as HTMLElement | null;
	}

	function tooltipCue(): HTMLElement | null {
		return document.body.querySelector(
			'[data-slot="tooltip-learn-more-cue"]'
		) as HTMLElement | null;
	}

	async function openTooltip(trigger: Element): Promise<HTMLElement> {
		await userEvent.hover(trigger);
		return await waitFor(() => {
			const content = tooltipContent();
			if (!content) throw new Error('tooltip content not yet rendered');
			return content;
		});
	}

	const { Story } = defineMeta({
		title: 'Shared/Tooltip',
		component: Tooltip,
		tags: ['autodocs'],
		argTypes: {
			text: { control: 'text' },
			slug: { control: 'text' },
			placement: {
				control: 'select',
				options: ['top', 'bottom', 'left', 'right']
			},
			delay: { control: 'number' },
			disabled: { control: 'boolean' }
		}
	});
</script>

<Story
	name="IconButton"
	tags={['interactive']}
	args={{ text: 'Copy to clipboard', delay: 0 }}
	play={async ({ canvasElement }) => {
		const trigger = canvasElement.querySelector(
			'[data-slot="tooltip-trigger"]'
		) as HTMLButtonElement | null;
		expect(trigger).not.toBeNull();
		expect(trigger).toHaveAttribute('aria-label', 'Copy to clipboard');
		await openTooltip(trigger!);
		expect(tooltipContent()?.textContent?.trim()).toBe('Copy to clipboard');
		expect(tooltipCue()).toBeNull();
	}}
>
	{#snippet template(args)}
		<div style="padding: 60px;">
			<Tooltip
				text={args.text!}
				slug={args.slug}
				placement={args.placement}
				delay={args.delay}
				disabled={args.disabled}
			>
				<Copy class="h-3.5 w-3.5" aria-hidden="true" />
			</Tooltip>
		</div>
	{/snippet}
</Story>

<Story
	name="NumericUnit"
	tags={['interactive']}
	args={{ text: 'milliseconds — wall-clock proxy latency', delay: 0 }}
	play={async ({ canvasElement }) => {
		const trigger = canvasElement.querySelector(
			'[data-slot="tooltip-trigger"]'
		) as HTMLButtonElement | null;
		expect(trigger).not.toBeNull();
		expect(trigger).toHaveAttribute('aria-label', 'milliseconds — wall-clock proxy latency');
		await openTooltip(trigger!);
		expect(tooltipContent()?.textContent?.trim()).toBe('milliseconds — wall-clock proxy latency');
	}}
>
	{#snippet template(args)}
		<div style="padding: 60px;">
			<span class="font-mono text-sm text-[var(--text-1)]">
				42<Tooltip
					text={args.text!}
					slug={args.slug}
					placement={args.placement}
					delay={args.delay}
					disabled={args.disabled}
				>
					<span
						class="ml-0.5 cursor-help text-[var(--text-3)] underline decoration-dotted decoration-1 underline-offset-2"
					>
						ms
					</span>
				</Tooltip>
			</span>
		</div>
	{/snippet}
</Story>

<Story
	name="WithSlugLink"
	args={{
		text: 'A captured system-prompt structure — click for full definition.',
		slug: 'composition',
		delay: 0
	}}
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const link = canvasElement.querySelector(
			'[data-slot="tooltip-trigger-link"]'
		) as HTMLAnchorElement | null;
		expect(link).not.toBeNull();
		expect(link?.getAttribute('href')).toBe('/help/glossary#composition');
		expect(link?.getAttribute('target')).toBe('_blank');
		expect(link?.getAttribute('rel')).toContain('noopener');
		await openTooltip(link!);
		expect(tooltipContent()?.textContent?.trim()).toBe(
			'A captured system-prompt structure — click for full definition.'
		);
		expect(tooltipCue()).not.toBeNull();
	}}
>
	{#snippet template(args)}
		<div style="padding: 60px;">
			<Tooltip
				text={args.text!}
				slug={args.slug}
				placement={args.placement}
				delay={args.delay}
				disabled={args.disabled}
			>
				<span
					class="inline-flex items-center gap-1 rounded border border-[var(--border-default)] bg-[var(--surface-2)] px-2 py-0.5 text-xs text-[var(--text-1)]"
				>
					<span>composition</span>
					<ExternalLink class="h-3 w-3 text-[var(--interactive-active-fg)]" aria-hidden="true" />
				</span>
			</Tooltip>
		</div>
	{/snippet}
</Story>

<Story
	name="CustomTrigger"
	tags={['interactive']}
	args={{ text: 'Pin this session — exempt from TTL eviction', delay: 0 }}
	play={async ({ canvasElement }) => {
		const trigger = canvasElement.querySelector(
			'[data-slot="tooltip-trigger"]'
		) as HTMLButtonElement | null;
		expect(trigger).not.toBeNull();
		expect(trigger).toHaveAttribute('data-tooltip-trigger');
		expect(trigger).toHaveAttribute('aria-label', 'Pin session');
		await openTooltip(trigger!);
		expect(tooltipContent()?.textContent?.trim()).toBe(
			'Pin this session — exempt from TTL eviction'
		);
		expect(tooltipCue()).toBeNull();
	}}
>
	{#snippet template(args)}
		<div style="padding: 60px;">
			<Tooltip
				text={args.text!}
				slug={args.slug}
				placement={args.placement}
				delay={args.delay}
				disabled={args.disabled}
			>
				{#snippet child({ props })}
					<button
						{...props}
						type="button"
						class="inline-flex h-6 w-6 items-center justify-center rounded text-[var(--text-3)] outline-none transition-colors hover:text-[var(--text-strong)] focus-visible:ring-1 focus-visible:ring-[var(--interactive-active-fg)]"
						aria-label="Pin session"
					>
						<Pin class="h-3.5 w-3.5" aria-hidden="true" />
					</button>
				{/snippet}
			</Tooltip>
		</div>
	{/snippet}
</Story>

<Story
	name="Disabled"
	tags={['interactive']}
	args={{ text: 'Should never appear', disabled: true, delay: 0 }}
	play={async ({ canvasElement }) => {
		const trigger = canvasElement.querySelector(
			'[data-slot="tooltip-trigger"]'
		) as HTMLButtonElement | null;
		expect(trigger).not.toBeNull();
		await userEvent.hover(trigger!);
		expect(tooltipContent()).toBeNull();
		expect(tooltipCue()).toBeNull();
	}}
>
	{#snippet template(args)}
		<div style="padding: 60px;">
			<Tooltip
				text={args.text!}
				slug={args.slug}
				placement={args.placement}
				delay={args.delay}
				disabled={args.disabled}
			>
				<span
					class="rounded border border-[var(--border-default)] px-2 py-1 text-xs text-[var(--text-3)]"
				>
					tooltip disabled
				</span>
			</Tooltip>
		</div>
	{/snippet}
</Story>
