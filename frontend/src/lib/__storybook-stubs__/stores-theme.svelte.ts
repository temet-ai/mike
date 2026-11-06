import { DEFAULT_THEME, isThemeId, type ThemeId } from '$lib/themes';

class ThemeStoreStub {
	current = $state<ThemeId>(DEFAULT_THEME);

	set(id: ThemeId): void {
		if (!isThemeId(id)) return;
		this.current = id;
	}
}

export const themeStore = new ThemeStoreStub();
