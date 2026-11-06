<script lang="ts">
	import { Info } from '@lucide/svelte';
	import * as Popover from '$lib/components/ui/popover';
	import { helpFor } from '$lib/help/helpContent';
	import { resolve } from '$app/paths';

	interface Props {
		slug: string;
		term?: string;
		definition?: string;
		ariaLabel?: string;
	}

	const { slug, term, definition, ariaLabel }: Props = $props();

	const entry = $derived(helpFor(slug));
	const resolvedTerm = $derived(term ?? entry?.term ?? slug);
	const resolvedDefinition = $derived(
		definition ?? entry?.long ?? `Definition unavailable. See /help/glossary#${slug}.`
	);
	const triggerLabel = $derived(ariaLabel ?? `What is ${resolvedTerm}?`);
	const learnMoreHref = $derived(`/help/glossary#${slug}`);
</script>

<Popover.Root>
	<Popover.Trigger
		class="help-hint-trigger inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-full text-[var(--text-3)] outline-none transition-colors hover:text-[var(--text-strong)] focus-visible:text-[var(--text-strong)] focus-visible:ring-1 focus-visible:ring-[var(--interactive-focus-ring)]"
		aria-label={triggerLabel}
		data-slot="help-hint-trigger"
	>
		<Info class="h-3.5 w-3.5" aria-hidden="true" />
	</Popover.Trigger>
	<Popover.Content class="help-hint-content max-w-[320px]" align="start" sideOffset={6}>
		<div class="flex flex-col gap-2" data-slot="help-hint-body">
			<h4
				class="text-xs font-semibold uppercase tracking-kpi text-[var(--text-3)]"
				data-slot="help-hint-term"
			>
				{resolvedTerm}
			</h4>
			<p
				class="text-[0.8125rem] leading-snug text-[var(--text-1)]"
				data-slot="help-hint-definition"
			>
				{resolvedDefinition}
			</p>
			<a
				class="inline-flex items-center gap-1 self-start text-xs text-[var(--interactive-active-fg)] no-underline outline-none transition-colors hover:underline focus-visible:underline"
				href={resolve(learnMoreHref as `/help/glossary#${string}`)}
				data-slot="help-hint-learn-more"
			>
				<span>Learn more →</span>
			</a>
		</div>
	</Popover.Content>
</Popover.Root>

<style>
	:global(.help-hint-trigger) {
		opacity: 0.6;
	}
	:global(.help-hint-trigger:hover),
	:global(.help-hint-trigger:focus-visible) {
		opacity: 1;
	}
</style>
