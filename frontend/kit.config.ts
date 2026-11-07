import adapter from '@sveltejs/adapter-static';
import type { Config } from '@sveltejs/kit/vite';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

// SvelteKit 3 takes the Svelte and Kit configuration through the `sveltekit()` plugin; a
// svelte.config.js beside vite.config.ts is an error (config_file_unsupported). Every
// `sveltekit()` call (the dev server and build in vite.config.ts, the unit and integration
// projects in vitest.config.ts) passes this object so they all see the same `$lib` alias and
// compiler options.
export const kitConfig: Config = {
	preprocess: vitePreprocess(),

	compilerOptions: {
		warningFilter: (warning) => warning.code !== 'custom_element_props_identifier'
	},

	adapter: adapter({
		pages: 'build',
		assets: 'build',
		fallback: 'index.html',
		precompress: false,
		strict: true
	}),

	// Kit 3 deprecates `$lib` in favour of `#lib`; the alias keeps the existing
	// `$lib/...` imports resolving until they are migrated.
	alias: { $lib: 'src/lib' },

	prerender: { entries: [] }
};
