import { defineConfig, configDefaults } from 'vitest/config';
import { sveltekit } from '@sveltejs/kit/vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { storybookTest } from '@storybook/addon-vitest/vitest-plugin';
import { playwright } from '@vitest/browser-playwright';
import tailwindcss from '@tailwindcss/vite';
import path from 'node:path';
import fs from 'node:fs';
import { fileURLToPath } from 'node:url';
import { libStubAliases } from './.storybook/stub-aliases';
import { kitConfig } from './kit.config';

const dirname = path.dirname(fileURLToPath(import.meta.url));

const STORYBOOK_THEME = process.env.STORYBOOK_THEME ?? '';

function resolveChromiumExecutable(): string | undefined {
	const explicit = process.env.PLAYWRIGHT_CHROMIUM_PATH;
	if (explicit && fs.existsSync(explicit)) return explicit;
	const root = process.env.PLAYWRIGHT_BROWSERS_PATH;
	if (!root || !fs.existsSync(root)) return undefined;
	const dir = fs
		.readdirSync(root)
		.filter((d) => /^chromium-\d+$/.test(d))
		.sort((a, b) => Number(b.split('-')[1]) - Number(a.split('-')[1]))[0];
	if (!dir) return undefined;
	const bin = path.join(root, dir, 'chrome-linux', 'chrome');
	return fs.existsSync(bin) ? bin : undefined;
}

const CHROMIUM_EXECUTABLE = resolveChromiumExecutable();

export default defineConfig({
	test: {
		testTimeout: 30000,
		hookTimeout: 30000,
		projects: [
			{
				plugins: [sveltekit(kitConfig)],
				resolve: {
					conditions: ['browser']
				},
				test: {
					name: 'unit',
					testTimeout: 30000,
					hookTimeout: 30000,
					include: ['src/**/*.{test,spec}.{js,ts}'],
					exclude: [...configDefaults.exclude, 'src/**/*.integration.{test,spec}.{js,ts}']
				}
			},
			{
				plugins: [sveltekit(kitConfig)],
				resolve: {
					conditions: ['browser']
				},
				test: {
					name: 'integration',
					testTimeout: 30000,
					hookTimeout: 30000,
					include: ['src/**/*.integration.{test,spec}.{js,ts}'],
					setupFiles: ['./src/lib/test-support/setup-integration.ts']
				}
			},
			{
				resolve: {
					alias: [
						...libStubAliases(dirname),
						{ find: '$lib', replacement: path.join(dirname, 'src/lib') },
						{
							find: '$app/env',
							replacement: path.join(dirname, 'src/lib/__storybook-stubs__/app-env.ts')
						},
						{
							find: '$app/navigation',
							replacement: path.join(dirname, 'src/lib/__storybook-stubs__/app-navigation.ts')
						},
						{
							find: '$app/state',
							replacement: path.join(dirname, 'src/lib/__storybook-stubs__/app-state.ts')
						},
						{
							find: '$app/forms',
							replacement: path.join(dirname, 'src/lib/__storybook-stubs__/app-forms.ts')
						},
						{
							find: '$app/paths',
							replacement: path.join(dirname, 'src/lib/__storybook-stubs__/app-paths.ts')
						}
					]
				},
				plugins: [
					tailwindcss(),
					svelte(),
					storybookTest({ configDir: path.join(dirname, '.storybook') })
				],
				define: {
					'import.meta.env.STORYBOOK_THEME': JSON.stringify(STORYBOOK_THEME),
					'import.meta.env.STORYBOOK_DISABLE_MOTION': JSON.stringify('true'),
					'import.meta.env.STORYBOOK_UPDATE_SNAPSHOTS': JSON.stringify(
						process.env.VITEST_UPDATE_SNAPSHOTS === '1' ? '1' : ''
					)
				},
				optimizeDeps: {
					include: [
						'storybook/test',
						'@storybook/addon-themes',
						'@lucide/svelte',
						'bits-ui',
						'tailwind-merge',
						'tailwind-variants'
					]
				},
				test: {
					name: 'storybook',
					testTimeout: 30000,
					hookTimeout: 30000,
					browser: {
						enabled: true,
						headless: true,
						provider: playwright({
							actionTimeout: 30000,
							...(CHROMIUM_EXECUTABLE
								? { launchOptions: { executablePath: CHROMIUM_EXECUTABLE } }
								: {})
						}),
						instances: [{ browser: 'chromium' }],
						screenshotDirectory: path.join(dirname, 'test-results', 'screenshots')
					}
				}
			}
		]
	}
});
