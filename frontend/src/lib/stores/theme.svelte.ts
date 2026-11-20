import { DEFAULT_THEME, THEME_STORAGE_KEY, isThemeId, type ThemeId } from '$lib/themes';

function readInitialTheme(): ThemeId {
	if (typeof document === 'undefined') return DEFAULT_THEME;

	const fromDom = document.documentElement.dataset.theme;
	if (isThemeId(fromDom)) return fromDom;

	try {
		const stored = localStorage.getItem(THEME_STORAGE_KEY);
		if (isThemeId(stored)) return stored;
	} catch {
		// Storage is unavailable (private mode or full); the value is simply not kept.
	}

	return DEFAULT_THEME;
}

class ThemeStore {
	current = $state<ThemeId>(readInitialTheme());

	set(id: ThemeId): void {
		if (!isThemeId(id)) return;
		this.current = id;
		if (typeof document !== 'undefined') {
			document.documentElement.dataset.theme = id;
		}
		try {
			localStorage.setItem(THEME_STORAGE_KEY, id);
		} catch {
			// Storage is unavailable (private mode or full); the value is simply not kept.
		}
	}
}

export const themeStore = new ThemeStore();
