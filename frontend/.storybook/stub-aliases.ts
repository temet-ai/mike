import path from 'node:path';

export function libStubAliases(root: string): { find: string | RegExp; replacement: string }[] {
	const stub = (file: string) => path.join(root, 'src/lib/__storybook-stubs__', file);
	return [
		{ find: '$lib/ws.svelte', replacement: stub('ws.svelte.ts') },
		{
			find: '$lib/stores/timelineScroll.svelte',
			replacement: stub('stores-timeline-scroll.svelte.ts')
		},
		{ find: '$lib/stores/shellState.svelte', replacement: stub('stores-shell-state.svelte.ts') },
		{ find: '$lib/stores/filterRules.svelte', replacement: stub('stores-filter.svelte.ts') },
		{ find: '$lib/stores/diagnostics.svelte', replacement: stub('stores-diagnostics.svelte.ts') },
		{ find: '$lib/stores/findings.svelte', replacement: stub('stores-findings.svelte.ts') },
		{ find: '$lib/stores/theme.svelte', replacement: stub('stores-theme.svelte.ts') },
		{ find: /^\$lib\/api\/(.+)$/, replacement: path.join(root, 'src/lib/api/$1.ts') },
		{ find: '$lib/api', replacement: stub('api.ts') }
	];
}
