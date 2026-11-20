const UPDATE_SNAPSHOTS = import.meta.env.STORYBOOK_UPDATE_SNAPSHOTS === '1';

/** The part of the browser locator used here; its typed options omit `save` and `path`. */
interface ScreenshotLocator {
	screenshot(options: Record<string, unknown>): Promise<unknown>;
}

export async function captureStorySnapshot(
	canvasElement: HTMLElement,
	snapshotName: string,
	options: { selector?: string } = {}
): Promise<void> {
	let page: (typeof import('@vitest/browser/context'))['page'];
	try {
		({ page } = await import('@vitest/browser/context'));
	} catch {
		return;
	}

	if (typeof document !== 'undefined' && document.fonts?.ready) {
		await document.fonts.ready;
	}

	const safeName = snapshotName.replace(/[^a-zA-Z0-9_/-]/g, '_');
	const committedPath = `__snapshots__/${safeName}.png`;

	const targetElement = options.selector
		? (canvasElement.querySelector(options.selector) ?? canvasElement)
		: canvasElement;

	const locator = page.elementLocator(targetElement as Element) as unknown as ScreenshotLocator;

	if (UPDATE_SNAPSHOTS) {
		await locator.screenshot({
			path: committedPath,
			save: true,
			animations: 'disabled',
			caret: 'hide',
			timeout: 30000
		});
		return;
	}

	await locator.screenshot({
		save: false,
		animations: 'disabled',
		caret: 'hide',
		timeout: 30000
	});
}
