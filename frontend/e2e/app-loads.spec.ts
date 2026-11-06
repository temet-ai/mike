import { test, expect } from '@playwright/test';

test('SPA root serves the built app and renders the inspector shell', async ({ page }) => {
	const response = await page.goto('/');
	expect(response?.ok(), 'preview server should return a 2xx for /').toBeTruthy();

	await expect(page).toHaveTitle(/mike/i);

	await expect(page.locator('body')).not.toBeEmpty();
});
