import { expect, screen, waitFor } from 'storybook/test';

/**
 * The open dialog, once it has finished fading in. A dialog is portalled out of the story's
 * canvas and starts transparent, so a test must wait for it to show before it asserts on it.
 */
export async function openDialog(name?: string): Promise<HTMLElement> {
	const dialog = await screen.findByRole('dialog', name === undefined ? undefined : { name });
	await waitFor(() => expect(dialog).toBeVisible());
	return dialog;
}
