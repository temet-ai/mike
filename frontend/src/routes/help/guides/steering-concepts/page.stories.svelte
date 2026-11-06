<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { fn, expect, within, userEvent } from 'storybook/test';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import Page from './+page.svelte';
	import GuideLayout from '$lib/components/help/GuideLayout.svelte';

	const { Story } = defineMeta({
		title: 'Routes/Help/Guides/SteeringConcepts',
		component: Page,
		tags: ['autodocs'],
		parameters: {
			layout: 'fullscreen',
			docs: {
				description: {
					component:
						'Static help-guide page explaining how steering works: lifecycle, authority levels ' +
						'(supply_context / redirect / command), mode (augment vs drive), enrolment, and a ' +
						'getting-started checklist. Rendered inside GuideLayout with a six-item table of contents.'
				}
			}
		}
	});

	void fn;
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const pageTitle = canvas.getByRole('heading', { level: 1, name: 'How steering works' });
		await expect(pageTitle.textContent?.trim()).toBe('How steering works');
	}}
>
	{#snippet template()}
		<Page />
	{/snippet}
</Story>

<Story
	name="DomStructure"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const pageTitle = canvas.getByRole('heading', { level: 1, name: 'How steering works' });
		await expect(pageTitle.textContent?.trim()).toBe('How steering works');

		const expectedTocLinks = [
			{ label: 'What is steering?', href: '#what-is-steering' },
			{ label: 'The lifecycle', href: '#the-lifecycle' },
			{ label: 'Authority levels', href: '#authority' },
			{ label: 'Mode: augment vs drive', href: '#mode' },
			{ label: 'Enrolment', href: '#enrolment' },
			{ label: 'Getting started', href: '#getting-started-now' }
		];
		for (const [index, { label, href }] of expectedTocLinks.entries()) {
			const link = canvas.getByRole('link', { name: label });
			const step = link.querySelector('.step-number');
			await expect(step).toHaveAttribute('aria-hidden', 'true');
			await expect(step?.textContent?.trim()).toBe(String(index + 1).padStart(2, '0'));
			await expect(link.textContent?.replace(/\s+/g, ' ').trim()).toBe(
				`${String(index + 1).padStart(2, '0')} ${label}`
			);
			await expect(link.getAttribute('href')).toBe(href);
		}

		const firstTocLink = canvas.getByRole('link', { name: 'What is steering?' });
		await expect(firstTocLink.getAttribute('href')).toBe('#what-is-steering');
		await userEvent.click(firstTocLink);
		await expect(firstTocLink.getAttribute('href')).toBe('#what-is-steering');

		const expectedSections = [
			{ id: 'what-is-steering', text: 'What is steering?' },
			{ id: 'the-lifecycle', text: 'The lifecycle' },
			{ id: 'authority', text: 'Authority levels' },
			{ id: 'mode', text: 'Mode: augment vs drive' },
			{ id: 'enrolment', text: 'Enrolment' },
			{ id: 'getting-started-now', text: 'Getting started' }
		];
		for (const { id, text } of expectedSections) {
			const heading = canvasElement.querySelector(`#${id}`);
			await expect(heading?.textContent?.trim(), `heading #${id} text`).toBe(text);
		}

		const tables = canvasElement.querySelectorAll('table');
		await expect(tables.length, 'two guide-table elements expected').toBe(2);

		const authorityTable = tables[0]!;
		const authorityCells = authorityTable.querySelectorAll('td:first-child code');
		const authorityLevels = Array.from(authorityCells).map((c) => c.textContent?.trim());
		await expect(authorityLevels).toEqual(['supply_context', 'redirect', 'command']);

		const modeTable = tables[1]!;
		const modeCells = modeTable.querySelectorAll('td:first-child code');
		const modes = Array.from(modeCells).map((c) => c.textContent?.trim());
		await expect(modes).toEqual(['augment', 'drive']);

		const calloutTitles = Array.from(canvasElement.querySelectorAll('.callout-title')).map((c) =>
			c.textContent?.trim()
		);
		await expect(calloutTitles, 'callout-title texts').toEqual([
			"Authority doesn't change how the message is delivered",
			"Drive doesn't replace the session's turn",
			'Start conservative'
		]);

		const guideIndexLinks = canvas.getAllByRole('link', { name: /All guides/i });
		await expect(guideIndexLinks.length).toBe(2);
		for (const link of guideIndexLinks) {
			await expect(link.getAttribute('href')).toBe('/help/guides');
		}

		const pills = canvasElement.querySelectorAll('.meta-pill');
		const pillTexts = Array.from(pills).map((p) => p.textContent?.trim());
		await expect(pillTexts).toContain('operator');
		await expect(pillTexts).toContain('5 min');
	}}
>
	{#snippet template()}
		<Page />
	{/snippet}
</Story>

<Story
	name="NoToc"
	tags={['design-exploration']}
	play={async ({ canvasElement }) => {
		const tocAside = canvasElement.querySelector('aside[aria-label="Section table of contents"]');
		await expect(tocAside, 'toc aside should be absent without toc prop').toBeNull();
		const article = canvasElement.querySelector('article.article');
		await expect(
			article?.classList.contains('has-toc'),
			'article should not have has-toc class'
		).toBe(false);
		const pageTitle = canvasElement.querySelector('h1');
		await expect(pageTitle?.textContent?.trim(), 'page title in no-toc variant').toBe(
			'No-TOC guide'
		);
	}}
>
	{#snippet template()}
		<GuideLayout
			title="No-TOC guide"
			summary="A guide page rendered without a table of contents. The article spans the full grid width via .article:not(.has-toc)."
			audience="developer"
			duration="2 min"
		>
			{#snippet children()}
				<h2 id="section-one">Section one</h2>
				<p>
					When the <code>toc</code> prop is absent or empty, GuideLayout omits the sticky sidebar entirely
					and the article column expands to fill the full content area.
				</p>
			{/snippet}
		</GuideLayout>
	{/snippet}
</Story>

<Story
	name="Snapshot"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'page/SteeringConcepts');
	}}
>
	{#snippet template()}
		<Page />
	{/snippet}
</Story>
