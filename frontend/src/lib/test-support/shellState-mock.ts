import { vi } from 'vitest';

export interface ShellStateMock {
	shellState: {
		filterText: string;
		paused: boolean;
		activeNav: string;
		helpDrawerOpen: boolean;
		commandPaletteOpen: boolean;
	};
	toggleCommandPalette: ReturnType<typeof vi.fn>;
	setCommandPaletteOpen: ReturnType<typeof vi.fn>;
}

export function makeShellMock(
	overrides: Partial<ShellStateMock['shellState']> = {}
): ShellStateMock {
	return {
		shellState: {
			filterText: '',
			paused: false,
			activeNav: 'live',
			helpDrawerOpen: false,
			commandPaletteOpen: false,
			...overrides
		},
		toggleCommandPalette: vi.fn(),
		setCommandPaletteOpen: vi.fn()
	};
}
