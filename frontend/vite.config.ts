import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';
import { kitConfig } from './kit.config';

const inspectBackend = process.env.INSPECT_BACKEND ?? 'http://127.0.0.1:9989';
const assistantBackend = process.env.PROMPT_ASSISTANT_BACKEND ?? 'http://127.0.0.1:9985';

const frontendPortRaw = process.env.MIKE_FRONTEND_PORT ?? '';
const parsedFrontendPort = Number.parseInt(frontendPortRaw, 10);
const frontendPort = Number.isFinite(parsedFrontendPort) && parsedFrontendPort > 0
	? parsedFrontendPort
	: 9990;

const frontendHost = (process.env.MIKE_FRONTEND_HOST ?? '').trim() || '127.0.0.1';

const inspectProxy = {
	// Must precede '/api' below: the prompt-assistant service (services/prompt-assistant)
	// owns this one path prefix, everything else under /api goes to the inspect backend.
	'/api/assistant': {
		target: assistantBackend,
		changeOrigin: true
	},
	'/api/ws': {
		target: inspectBackend,
		changeOrigin: true,
		ws: true
	},
	'/api': {
		target: inspectBackend,
		changeOrigin: true
	}
};

export default defineConfig({
	plugins: [
		tailwindcss(),
		// SvelteKit 3 takes the Svelte and Kit configuration here; see kit.config.ts.
		sveltekit(kitConfig)
	],

	server: {
		host: frontendHost,
		port: frontendPort,
		strictPort: true,
		proxy: inspectProxy,
		watch: {
			ignored: [
				'**/*.stories.svelte',
				'**/__snapshots__/**',
				'**/__screenshots__/**',
				'**/test-results/**'
			]
		},
		allowedHosts: frontendHost === '127.0.0.1' ? undefined : true
	},

	preview: {
		host: frontendHost,
		port: frontendPort,
		strictPort: true,
		proxy: inspectProxy,
		allowedHosts: frontendHost === '127.0.0.1' ? undefined : true
	}
});
