import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

const config = {
	preprocess: vitePreprocess(),

	compilerOptions: {
		warningFilter: (warning) =>
			warning.code !== 'custom_element_props_identifier'
	},

	kit: {
		adapter: adapter({
			pages: 'build',
			assets: 'build',
			fallback: 'index.html',
			precompress: false,
			strict: true
		}),

		prerender: { entries: [] }
	}
};

export default config;
