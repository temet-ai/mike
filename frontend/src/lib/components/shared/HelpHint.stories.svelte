<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor } from 'storybook/test';
	import HelpHint from './HelpHint.svelte';
	import { helpFor } from '$lib/help/helpContent';

	interface HelpHintProps {
		slug: string;
		term?: string;
		definition?: string;
		ariaLabel?: string;
	}

	function buildHelpHintProps(overrides: Partial<HelpHintProps> = {}): HelpHintProps {
		return {
			slug: 'session',
			...overrides
		};
	}

	function helpSlot(slot: string): HTMLElement | null {
		return document.body.querySelector(`[data-slot="${slot}"]`) as HTMLElement | null;
	}

	async function openHelpHint(trigger: Element): Promise<HTMLElement> {
		await userEvent.click(trigger);
		return await waitFor(() => {
			const term = helpSlot('help-hint-term');
			if (!term) throw new Error('help hint popover not yet rendered');
			return term;
		});
	}

	const { Story } = defineMeta({
		title: 'Shared/HelpHint',
		component: HelpHint,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	tags={['interactive']}
	args={buildHelpHintProps({ slug: 'session' })}
	play={async ({ canvasElement }) => {
		const trigger = canvasElement.querySelector(
			'[data-slot="help-hint-trigger"]'
		) as HTMLButtonElement | null;
		expect(trigger).not.toBeNull();
		expect(trigger).toHaveAttribute('aria-label', 'What is Session?');
		await openHelpHint(trigger!);
		expect(helpSlot('help-hint-term')?.textContent?.trim()).toBe('Session');
		expect(helpSlot('help-hint-definition')?.textContent?.trim()).toBe(helpFor('session')?.long);
		expect(helpSlot('help-hint-learn-more')?.getAttribute('href')).toBe('/help/glossary#session');
	}}
>
	{#snippet template(args)}
		<div class="flex items-center gap-2 p-4">
			<span class="text-sm text-[var(--text-1)]">Session</span>
			<HelpHint {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="InlineOverride"
	tags={['interactive']}
	args={buildHelpHintProps({
		slug: 'composition',
		term: 'Composition',
		definition: 'A captured system-prompt composition extracted from a Claude Code request.'
	})}
	play={async ({ canvasElement }) => {
		const trigger = canvasElement.querySelector(
			'[data-slot="help-hint-trigger"]'
		) as HTMLButtonElement | null;
		expect(trigger).not.toBeNull();
		expect(trigger).toHaveAttribute('aria-label', 'What is Composition?');
		await openHelpHint(trigger!);
		expect(helpSlot('help-hint-term')?.textContent?.trim()).toBe('Composition');
		expect(helpSlot('help-hint-definition')?.textContent?.trim()).toBe(
			'A captured system-prompt composition extracted from a Claude Code request.'
		);
		expect(helpSlot('help-hint-learn-more')?.getAttribute('href')).toBe(
			'/help/glossary#composition'
		);
	}}
>
	{#snippet template(args)}
		<div class="flex items-center gap-2 p-4">
			<span class="text-sm text-[var(--text-1)]">Composition</span>
			<HelpHint {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="CustomAriaLabel"
	tags={['interactive']}
	args={buildHelpHintProps({
		slug: 'anomaly',
		ariaLabel: 'About anomalies'
	})}
	play={async ({ canvasElement }) => {
		const trigger = canvasElement.querySelector(
			'[data-slot="help-hint-trigger"]'
		) as HTMLButtonElement | null;
		expect(trigger).not.toBeNull();
		expect(trigger).toHaveAttribute('aria-label', 'About anomalies');
	}}
>
	{#snippet template(args)}
		<div class="flex items-center gap-2 p-4">
			<span class="text-sm text-[var(--text-1)]">Anomaly</span>
			<HelpHint {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="WeaselPattern"
	tags={['interactive']}
	args={buildHelpHintProps({
		slug: 'weasel-pattern',
		term: 'Weasel pattern',
		definition: 'Behavioral patterns the proxy detects in captured request bodies.'
	})}
	play={async ({ canvasElement }) => {
		const trigger = canvasElement.querySelector(
			'[data-slot="help-hint-trigger"]'
		) as HTMLButtonElement | null;
		expect(trigger).not.toBeNull();
		expect(trigger).toHaveAttribute('aria-label', 'What is Weasel pattern?');
		await openHelpHint(trigger!);
		expect(helpSlot('help-hint-term')?.textContent?.trim()).toBe('Weasel pattern');
		expect(helpSlot('help-hint-definition')?.textContent?.trim()).toBe(
			'Behavioral patterns the proxy detects in captured request bodies.'
		);
		expect(helpSlot('help-hint-learn-more')?.getAttribute('href')).toBe(
			'/help/glossary#weasel-pattern'
		);
	}}
>
	{#snippet template(args)}
		<div class="flex items-center gap-2 p-4">
			<span class="text-sm text-[var(--text-1)]">Weasel pattern</span>
			<HelpHint {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="UnknownSlug"
	tags={['interactive']}
	args={buildHelpHintProps({ slug: 'unknown-term-xyz' })}
	play={async ({ canvasElement }) => {
		const trigger = canvasElement.querySelector(
			'[data-slot="help-hint-trigger"]'
		) as HTMLButtonElement | null;
		expect(trigger).not.toBeNull();
		expect(trigger).toHaveAttribute('aria-label', 'What is unknown-term-xyz?');
		await openHelpHint(trigger!);
		expect(helpSlot('help-hint-term')?.textContent?.trim()).toBe('unknown-term-xyz');
		expect(helpSlot('help-hint-definition')?.textContent?.trim()).toBe(
			'Definition unavailable. See /help/glossary#unknown-term-xyz.'
		);
		expect(helpSlot('help-hint-learn-more')?.getAttribute('href')).toBe(
			'/help/glossary#unknown-term-xyz'
		);
	}}
>
	{#snippet template(args)}
		<div class="flex items-center gap-2 p-4">
			<span class="text-sm text-[var(--text-1)]">Unknown term</span>
			<HelpHint {...args} />
		</div>
	{/snippet}
</Story>
