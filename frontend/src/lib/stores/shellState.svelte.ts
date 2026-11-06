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

const LS_KEY = 'mike:shell-columns:v1';

interface PersistedColumns {
	sessionsCollapsed?: boolean;
	requestsCollapsed?: boolean;
	sessionsWidth?: number;
	requestsWidth?: number;
}

function loadPersisted(): PersistedColumns {
	if (typeof window === 'undefined' || typeof localStorage === 'undefined') return {};
	try {
		const raw = localStorage.getItem(LS_KEY);
		if (!raw) return {};
		const parsed = JSON.parse(raw) as unknown;
		if (parsed === null || typeof parsed !== 'object') return {};
		return parsed as PersistedColumns;
	} catch {
		return {};
	}
}

function persistColumns(): void {
	if (typeof window === 'undefined' || typeof localStorage === 'undefined') return;
	try {
		const payload: PersistedColumns = {
			sessionsCollapsed: shellState.sessionsCollapsed,
			requestsCollapsed: shellState.requestsCollapsed,
			sessionsWidth: shellState.sessionsWidth,
			requestsWidth: shellState.requestsWidth
		};
		localStorage.setItem(LS_KEY, JSON.stringify(payload));
	} catch {}
}

function clamp(n: number, min: number, max: number): number {
	if (!Number.isFinite(n)) return min;
	return Math.min(max, Math.max(min, Math.round(n)));
}

const persisted = loadPersisted();

export const shellState = $state<ShellState>({
	filterText: '',
	paused: false,
	activeNav: 'live',
	helpDrawerOpen: false,
	commandPaletteOpen: false,
	drawerKind: null,
	drawerBandId: null,
	sessionsCollapsed: persisted.sessionsCollapsed === true,
	requestsCollapsed: persisted.requestsCollapsed === true,
	sessionsWidth: clamp(
		persisted.sessionsWidth ?? SESSIONS_WIDTH_DEFAULT,
		SESSIONS_WIDTH_MIN,
		SESSIONS_WIDTH_MAX
	),
	requestsWidth: clamp(
		persisted.requestsWidth ?? REQUESTS_WIDTH_DEFAULT,
		REQUESTS_WIDTH_MIN,
		REQUESTS_WIDTH_MAX
	)
});

export function setSessionsCollapsed(collapsed: boolean): void {
	shellState.sessionsCollapsed = collapsed;
	persistColumns();
}

export function toggleSessionsCollapsed(): void {
	setSessionsCollapsed(!shellState.sessionsCollapsed);
}

export function setRequestsCollapsed(collapsed: boolean): void {
	shellState.requestsCollapsed = collapsed;
	persistColumns();
}

export function toggleRequestsCollapsed(): void {
	setRequestsCollapsed(!shellState.requestsCollapsed);
}

export function setSessionsWidth(width: number): void {
	shellState.sessionsWidth = clamp(width, SESSIONS_WIDTH_MIN, SESSIONS_WIDTH_MAX);
	persistColumns();
}

export function setRequestsWidth(width: number): void {
	shellState.requestsWidth = clamp(width, REQUESTS_WIDTH_MIN, REQUESTS_WIDTH_MAX);
	persistColumns();
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
