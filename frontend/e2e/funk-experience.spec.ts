import { expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
	await page.route('**/api/**', (route) =>
		route.fulfill({ status: 503, contentType: 'application/json', body: '{"error":"Offline"}' })
	);
});

test.describe('touch screens', () => {
	test.use({ isMobile: true, hasTouch: true });

	test('mobile navigation fits, restores focus and closes after navigation', async ({
		page
	}, testInfo) => {
		await page.setViewportSize({ width: 390, height: 844 });
		await page.goto('/help');
		const trigger = page.getByRole('button', { name: 'Open navigation', exact: true });
		await expect(trigger).toBeVisible();
		expect((await trigger.boundingBox())!.height).toBeGreaterThanOrEqual(44);
		await trigger.click();
		const navigation = page.getByRole('dialog', { name: 'Navigate', exact: true });
		await expect(navigation).toBeVisible();
		await expect(navigation).toHaveCSS('--aug-border-all', '1px');
		await navigation.evaluate(async (element) => {
			await Promise.all(element.getAnimations().map((animation) => animation.finished));
		});
		const bounds = await navigation.boundingBox();
		expect(bounds).not.toBeNull();
		expect(bounds!.x).toBeGreaterThanOrEqual(0);
		expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(391);
		expect(bounds!.height).toBeLessThanOrEqual(844);
		await page.screenshot({ path: testInfo.outputPath('mobile-navigation.png') });
		await page.keyboard.press('Escape');
		await expect(navigation).not.toBeVisible();
		await expect(trigger).toBeFocused();
		await trigger.click();
		await navigation.getByRole('link', { name: 'Projects', exact: true }).click();
		await expect(page).toHaveURL(/\/projects$/);
		await expect(navigation).not.toBeVisible();
		await expect(page.locator('main')).toHaveCount(1);
		expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(
			391
		);
	});

	test('command search has working motion and focus', async ({ page }, testInfo) => {
		await page.setViewportSize({ width: 390, height: 844 });
		await page.goto('/help');
		const trigger = page.getByRole('button', { name: 'Search pages and commands', exact: true });
		await expect(trigger).toBeVisible();
		expect((await trigger.boundingBox())!.height).toBeGreaterThanOrEqual(44);
		await trigger.click();
		const search = page.getByRole('combobox', { name: 'Search pages, sessions and commands' });
		await expect(search).toBeFocused();
		const dialog = page.locator('[data-slot="dialog-content"]');
		const close = page.getByRole('button', { name: 'Close command palette', exact: true });
		expect((await close.boundingBox())!.height).toBeGreaterThanOrEqual(44);
		expect((await close.boundingBox())!.width).toBeGreaterThanOrEqual(44);
		await expect(dialog).toHaveCSS('--aug-border-all', '1px');
		const name = await dialog.evaluate((element) => getComputedStyle(element).animationName);
		expect(name).toBe('funk-dialog-in');
		const bounds = await dialog.boundingBox();
		expect(bounds!.x).toBeGreaterThanOrEqual(0);
		expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(391);
		expect(bounds!.height).toBeLessThanOrEqual(844);
		await search.fill('Projects');
		await expect(page.getByRole('option', { name: /Projects Every codebase/ })).toBeVisible();
		await dialog.evaluate(async (element) => {
			await Promise.all(element.getAnimations().map((animation) => animation.finished));
		});
		await page.screenshot({ path: testInfo.outputPath('command-palette.png') });
		await page.keyboard.press('Escape');
		await expect(dialog).not.toBeVisible();
		await expect(trigger).toBeFocused();
	});
});

test('desktop keyboard entry and theme picker remain accessible', async ({ page }) => {
	await page.setViewportSize({ width: 1440, height: 900 });
	await page.goto('/help');
	await expect(page.getByRole('button', { name: 'Search pages and commands' })).toBeVisible();
	await page.keyboard.press('Tab');
	await expect(page.getByRole('link', { name: 'Skip to content' })).toBeFocused();
	await page.keyboard.press('Enter');
	await expect(page.locator('#main-content')).toBeFocused();
	await page.getByRole('button', { name: 'Choose colour theme' }).click();
	const glacier = page.getByRole('radio', { name: 'Glacier', exact: true });
	await glacier.check();
	await expect(page.locator('html')).toHaveAttribute('data-theme', 'glacier');
	await expect(glacier).toBeChecked();
	await page.keyboard.press('Escape');
	await expect(page.getByRole('button', { name: 'Choose colour theme' })).toBeFocused();
});

test('command search close button respects keyboard activation', async ({ page }) => {
	await page.goto('/help');
	const trigger = page.getByRole('button', { name: 'Search pages and commands', exact: true });
	await trigger.click();
	const search = page.getByRole('combobox', { name: 'Search pages, sessions and commands' });
	await search.fill('Projects');
	await expect(page.getByRole('option', { name: /Projects Every codebase/ })).toBeVisible();
	await page.keyboard.press('Tab');
	await expect(
		page.getByRole('button', { name: 'Close command palette', exact: true })
	).toBeFocused();
	await page.keyboard.press('Enter');
	await expect(page.locator('[data-slot="dialog-content"]')).not.toBeVisible();
	await expect(page).toHaveURL(/\/help$/);
	await expect(trigger).toBeFocused();
});
