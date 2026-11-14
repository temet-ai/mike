import { isNotDeployed } from '$lib/api/wright';

/**
 * A list the page keeps stable while it is open. Rows already on screen are repainted in place
 * when their data changes; rows that arrive passively wait in `held` until the operator asks
 * for them, so nothing is inserted, removed or reordered under the pointer.
 */

export type FeedKey = string | number;

export type FeedStatus = 'loading' | 'ready' | 'missing' | 'error';

export interface Feed<T> {
	rows: T[];
	held: T[];
	status: FeedStatus;
	error: string | null;
	/** Where the next page starts (an offset or an id, per route); null when there is no more. */
	cursor: number | null;
	total: number | null;
	loadingMore: boolean;
}

export interface FeedPage<T> {
	rows: T[];
	cursor: number | null;
	total?: number | null;
}

export function emptyFeed<T>(): Feed<T> {
	return {
		rows: [],
		held: [],
		status: 'loading',
		error: null,
		cursor: null,
		total: null,
		loadingMore: false
	};
}

export function errorMessage(err: unknown): string {
	return err instanceof Error ? err.message : String(err);
}

/** Rows updated in place from `fetched`, and the fetched rows that are not on screen yet. */
export function mergeInPlace<T>(
	current: T[],
	fetched: T[],
	key: (row: T) => FeedKey
): { rows: T[]; held: T[] } {
	const byKey = new Map(fetched.map((row) => [key(row), row]));
	const onScreen = new Set(current.map(key));
	return {
		rows: current.map((row) => byKey.get(key(row)) ?? row),
		held: fetched.filter((row) => !onScreen.has(key(row)))
	};
}

/** Rows after the operator asks for the held ones: sorted on the route's immutable key. */
export function withHeld<T>(rows: T[], held: T[], compare: (a: T, b: T) => number): T[] {
	return [...rows, ...held].sort(compare);
}

export function appendUnique<T>(rows: T[], more: T[], key: (row: T) => FeedKey): T[] {
	const onScreen = new Set(rows.map(key));
	return [...rows, ...more.filter((row) => !onScreen.has(key(row)))];
}

function settleFailure<T>(feed: Feed<T>, err: unknown): void {
	if (isNotDeployed(err)) {
		feed.status = 'missing';
		feed.error = null;
	} else {
		feed.status = 'error';
		feed.error = errorMessage(err);
	}
}

/** First load, retry and reconnect: the feed becomes exactly what the server holds. */
export async function fillFeed<T>(feed: Feed<T>, load: () => Promise<FeedPage<T>>): Promise<void> {
	try {
		const page = await load();
		feed.rows = page.rows;
		feed.held = [];
		feed.cursor = page.cursor;
		feed.total = page.total ?? null;
		feed.status = 'ready';
		feed.error = null;
	} catch (err) {
		settleFailure(feed, err);
	}
}

/** Passive refresh: update rows in place, hold the new ones. A failed refresh keeps what is shown. */
export async function refreshFeed<T>(
	feed: Feed<T>,
	load: () => Promise<FeedPage<T>>,
	key: (row: T) => FeedKey
): Promise<void> {
	if (feed.status === 'loading') return;
	try {
		const page = await load();
		const merged = mergeInPlace(feed.rows, page.rows, key);
		feed.rows = merged.rows;
		feed.held = merged.held;
		feed.total = page.total ?? feed.total;
		feed.status = 'ready';
		feed.error = null;
		if (feed.rows.length === 0 && merged.held.length > 0) {
			// Nothing was on screen to disturb: show what arrived.
			feed.rows = merged.held;
			feed.held = [];
			feed.cursor = page.cursor;
		}
	} catch (err) {
		if (feed.status !== 'ready') settleFailure(feed, err);
	}
}

export async function moreFeed<T>(
	feed: Feed<T>,
	load: (cursor: number) => Promise<FeedPage<T>>,
	key: (row: T) => FeedKey
): Promise<void> {
	if (feed.cursor === null || feed.loadingMore) return;
	feed.loadingMore = true;
	try {
		const page = await load(feed.cursor);
		feed.rows = appendUnique(feed.rows, page.rows, key);
		feed.cursor = page.cursor;
		feed.total = page.total ?? feed.total;
		feed.error = null;
	} catch (err) {
		feed.error = errorMessage(err);
	} finally {
		feed.loadingMore = false;
	}
}

export function showHeld<T>(feed: Feed<T>, compare: (a: T, b: T) => number): void {
	if (feed.held.length === 0) return;
	feed.rows = withHeld(feed.rows, feed.held, compare);
	feed.held = [];
}
