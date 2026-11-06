import type { StorybookConfig } from '@storybook/sveltekit';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { libStubAliases } from './stub-aliases.ts';
import tailwindcss from '@tailwindcss/vite';

const config: StorybookConfig = {
	stories: ['../src/**/*.stories.@(js|jsx|ts|tsx|svelte)'],
	staticDirs: ['../static'],
	addons: [
		'@storybook/addon-svelte-csf',
		'@storybook/addon-a11y',
		'@storybook/addon-themes',
		'@storybook/addon-vitest',
		'@storybook/addon-mcp'
	],
	framework: '@storybook/sveltekit',
	async viteFinal(viteConfig) {
		viteConfig.plugins = [tailwindcss(), ...(viteConfig.plugins ?? [])];
		const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
		const stubs = libStubAliases(root);
		viteConfig.resolve ??= {};
		const existing = viteConfig.resolve.alias;
		if (Array.isArray(existing)) {
			viteConfig.resolve.alias = [...stubs, ...existing];
		} else if (existing && typeof existing === 'object') {
			viteConfig.resolve.alias = [
				...stubs,
				...Object.entries(existing).map(([find, replacement]) => ({
					find,
					replacement: replacement as string
				}))
			];
		} else {
			viteConfig.resolve.alias = [...stubs];
		}
		return viteConfig;
	}
};

export default config;
