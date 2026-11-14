/**
 * The operator token Wright's page posts with. `mike wright pair --open` opens
 * `/wright#pair=<token>`; the page keeps the token in localStorage and removes it from the
 * address bar. Every storage access sits in try/catch: private windows and blocked site data
 * throw, and the page must still render (unpaired).
 */

export const PAIR_STORAGE_KEY = 'mike:wright:operator-token';
export const PAIR_COMMAND = 'mike wright pair --open';

export function readOperatorToken(): string | null {
	try {
		const value = localStorage.getItem(PAIR_STORAGE_KEY);
		return value !== null && value.length > 0 ? value : null;
	} catch {
		return null;
	}
}

export function storeOperatorToken(token: string): boolean {
	try {
		localStorage.setItem(PAIR_STORAGE_KEY, token);
		return true;
	} catch {
		return false;
	}
}

export function clearOperatorToken(): void {
	try {
		localStorage.removeItem(PAIR_STORAGE_KEY);
	} catch {
		// Nothing stored, nothing to clear.
	}
}

/** The token in a `#pair=<token>` fragment, or null when the fragment carries none. */
export function pairTokenFromHash(hash: string): string | null {
	const body = hash.startsWith('#') ? hash.slice(1) : hash;
	for (const part of body.split('&')) {
		if (!part.startsWith('pair=')) continue;
		let value: string;
		try {
			value = decodeURIComponent(part.slice('pair='.length)).trim();
		} catch {
			return null;
		}
		return value.length > 0 ? value : null;
	}
	return null;
}

/** The fragment without its `pair=` part, so a deep link such as `#problem-3` survives pairing. */
export function hashWithoutPair(hash: string): string {
	const body = hash.startsWith('#') ? hash.slice(1) : hash;
	const rest = body.split('&').filter((part) => !part.startsWith('pair='));
	return rest.length > 0 ? `#${rest.join('&')}` : '';
}
