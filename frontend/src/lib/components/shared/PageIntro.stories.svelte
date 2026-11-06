<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import PageIntro from './PageIntro.svelte';
	import HelpHint from './HelpHint.svelte';

	const { Story } = defineMeta({
		title: 'Shared/PageIntro',
		component: PageIntro,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const title = canvasElement.querySelector('[data-slot="page-intro-title"]');
		expect(title?.textContent?.trim()).toBe('Prompt management');

		const lede = canvasElement.querySelector('[data-slot="page-intro-lede"]');
		expect(lede?.textContent).toContain('System-prompt');
		expect(lede?.textContent).toContain('captured from the system block of recent requests.');

		expect(lede?.querySelector('[data-slot="help-hint-trigger"]')).not.toBeNull();

		const learnMore = canvasElement.querySelector('[data-slot="page-intro-learn-more"]');
		expect(learnMore?.getAttribute('href')).toBe('/help/glossary#composition');
		expect(learnMore?.textContent?.trim()).toBe('Learn more →');
	}}
>
	{#snippet template()}
		<div class="p-6">
			<PageIntro title="Prompt management" learnMoreHref="/help/glossary#composition">
				{#snippet children()}
					<span class="flex items-center gap-1.5">
						<span>System-prompt</span>
						<span class="inline-flex items-center gap-1">
							<span>compositions</span>
							<HelpHint slug="composition" />
						</span>
						<span>captured from the system block of recent requests.</span>
					</span>
				{/snippet}
			</PageIntro>
		</div>
	{/snippet}
</Story>
