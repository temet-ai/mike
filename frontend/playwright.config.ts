import { defineConfig } from '@playwright/test';

export default defineConfig({
	testDir: 'e2e',
	webServer: {
		command: 'pnpm run build && pnpm run preview',
		port: 4173,
		env: { MIKE_FRONTEND_PORT: '4173' },
		reuseExistingServer: !process.env.CI,
		timeout: 180_000
	},
	use: {
		baseURL: 'http://127.0.0.1:4173'
	}
});
