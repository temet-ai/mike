import { onNavigate } from '$app/navigation';
import { onDestroy } from 'svelte';

export function setupNavigationMotion(): void {
	let active: ViewTransition | undefined;
	onNavigate((navigation) => {
		if (
			!document.startViewTransition ||
			navigation.from?.url.pathname === navigation.to?.url.pathname
		)
			return;
		active?.skipTransition();
		return new Promise<void>((resolve) => {
			active = document.startViewTransition(async () => {
				resolve();
				await navigation.complete;
			});
			void active.ready.catch(resolve);
		});
	});
	onDestroy(() => active?.skipTransition());
}
