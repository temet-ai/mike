export type ShellNavItem = 'live' | 'sessions' | 'findings' | 'metrics' | 'replay' | 'settings';

export type DrawerKind = 'request' | 'response' | 'events' | 'findings';

export const DRAWER_KINDS: readonly DrawerKind[] = [
	'request',
	'response',
	'events',
	'findings'
] as const;

export function isDrawerKind(raw: string | null | undefined): raw is DrawerKind {
	return raw !== null && raw !== undefined && (DRAWER_KINDS as readonly string[]).includes(raw);
}

interface ShellState {
	filterText: string;
	paused: boolean;
	activeNav: ShellNavItem;
	helpDrawerOpen: boolean;
	commandPaletteOpen: boolean;
	drawerKind: DrawerKind | null;
	drawerBandId: number | null;
	sessionsCollapsed: boolean;
	requestsCollapsed: boolean;
	sessionsWidth: number;
	requestsWidth: number;
}

export const SESSIONS_WIDTH_DEFAULT = 260;
export const REQUESTS_WIDTH_DEFAULT = 380;
export const SESSIONS_WIDTH_MIN = 200;
export const SESSIONS_WIDTH_MAX = 520;
export const REQUESTS_WIDTH_MIN = 280;
export const REQUESTS_WIDTH_MAX = 640;
export const COLUMN_STUB_WIDTH = 28;

export const shellState: ShellState = $state({
	filterText: '',
	paused: false,
	activeNav: 'live',
	helpDrawerOpen: false,
	commandPaletteOpen: false,
	drawerKind: null,
	drawerBandId: null,
	sessionsCollapsed: false,
	requestsCollapsed: false,
	sessionsWidth: SESSIONS_WIDTH_DEFAULT,
	requestsWidth: REQUESTS_WIDTH_DEFAULT
});

export function setSessionsCollapsed(collapsed: boolean): void {
	shellState.sessionsCollapsed = collapsed;
}

export function toggleSessionsCollapsed(): void {
	shellState.sessionsCollapsed = !shellState.sessionsCollapsed;
}

export function setRequestsCollapsed(collapsed: boolean): void {
	shellState.requestsCollapsed = collapsed;
}

export function toggleRequestsCollapsed(): void {
	shellState.requestsCollapsed = !shellState.requestsCollapsed;
}

export function setSessionsWidth(width: number): void {
	shellState.sessionsWidth = width;
}

export function setRequestsWidth(width: number): void {
	shellState.requestsWidth = width;
}

export function setCommandPaletteOpen(open: boolean): void {
	shellState.commandPaletteOpen = open;
}

export function toggleCommandPalette(): void {
	shellState.commandPaletteOpen = !shellState.commandPaletteOpen;
}

export function openDrawer(bandId: number, kind: DrawerKind): void {
	shellState.drawerBandId = bandId;
	shellState.drawerKind = kind;
}

export function closeDrawer(): void {
	shellState.drawerBandId = null;
	shellState.drawerKind = null;
}
