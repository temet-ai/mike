export const THEMES = [
	{ id: 'forge', label: 'Forge' },
	{ id: 'glacier', label: 'Glacier' },
	{ id: 'void', label: 'Void' },
	{ id: 'phosphor', label: 'Phosphor' },
	{ id: 'crimson', label: 'Crimson' }
] as const;

export type ThemeId = (typeof THEMES)[number]['id'];

export const DEFAULT_THEME: ThemeId = 'forge';

export const THEME_STORAGE_KEY = 'temet-theme';

export function isThemeId(value: string | null | undefined): value is ThemeId {
	return value != null && THEMES.some((t) => t.id === value);
}
