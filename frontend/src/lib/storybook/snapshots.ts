const UPDATE_SNAPSHOTS = import.meta.env.STORYBOOK_UPDATE_SNAPSHOTS === '1';

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

	const locator = page.elementLocator(targetElement as Element);

	if (UPDATE_SNAPSHOTS) {
		await (locator as any).screenshot({
			path: committedPath,
			save: true,
			animations: 'disabled',
			caret: 'hide',
			timeout: 30000
		});
		return;
	}

	await (locator as any).screenshot({
		save: false,
		animations: 'disabled',
		caret: 'hide',
		timeout: 30000
	});
}
