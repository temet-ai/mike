<script lang="ts">
	import DOMPurify from 'dompurify';
	import { resolve } from '$app/paths';
	import GuideLayout from '$lib/components/help/GuideLayout.svelte';
	import { Button } from '$lib/components/ui/button';
	import { renderDocument } from '$lib/help/renderDocument';
	import { PROJECT_DOCS } from '$lib/help/externalDocs';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();
	const rendered = $derived(renderDocument(data.content));
	const content = $derived(DOMPurify.sanitize(rendered.html));
</script>

<svelte:head><title>{data.document.label} — mike help</title></svelte:head>

<GuideLayout
	title={data.document.label}
	summary={data.document.description}
	audience="installed users"
	duration="5 min"
	toc={rendered.sections}
>
	<nav aria-label="Installed documentation" class="flex flex-wrap gap-2">
		{#each PROJECT_DOCS as document (document.href)}
			<Button
				href={resolve('/help/docs/[slug]', { slug: document.href.split('/').at(-1) ?? '' })}
				variant="outline"
				aria-current={document.href === data.document.href ? 'page' : undefined}
				>{document.label}</Button
			>
		{/each}
	</nav>
	<!-- eslint-disable-next-line svelte/no-at-html-tags -- Bundled Markdown disables raw HTML; DOMPurify sanitizes the renderer output. -->
	<div class="flex flex-col gap-4" data-slot="packaged-document">{@html content}</div>
</GuideLayout>
