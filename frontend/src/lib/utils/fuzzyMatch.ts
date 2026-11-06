export interface FuzzyResult<T> {
	item: T;
	score: number;
	matchedIndexes: readonly number[];
}

export function score(query: string, haystack: string): FuzzyResult<null> | null {
	const q = query.toLowerCase().trim();
	const h = haystack.toLowerCase();
	if (q.length === 0) return { item: null, score: 0, matchedIndexes: [] };
	if (h.length === 0) return null;

	const directIdx = h.indexOf(q);
	if (directIdx !== -1) {
		const positionBonus = directIdx === 0 ? 200 : 100 - Math.min(directIdx, 50);
		return {
			item: null,
			score: 1000 + positionBonus + q.length,
			matchedIndexes: Array.from({ length: q.length }, (_, i) => directIdx + i)
		};
	}

	const matched: number[] = [];
	let consecutiveBonus = 0;
	let total = 0;
	let hi = 0;
	for (let qi = 0; qi < q.length; qi++) {
		const qc = q[qi];
		let found = -1;
		while (hi < h.length) {
			if (h[hi] === qc) {
				found = hi;
				break;
			}
			hi++;
		}
		if (found === -1) return null;

		matched.push(found);
		total += 10;

		if (matched.length >= 2 && matched[matched.length - 2] === found - 1) {
			consecutiveBonus += 5;
			total += consecutiveBonus;
		} else {
			consecutiveBonus = 0;
		}

		if (found === 0) {
			total += 20;
		} else {
			const prev = h[found - 1];
			if (prev === ' ' || prev === '-' || prev === '/' || prev === ':' || prev === '_') {
				total += 15;
			}
		}

		hi = found + 1;
	}

	return { item: null, score: total, matchedIndexes: matched };
}

export function match<T>(
	query: string,
	items: readonly T[],
	getHaystack: (item: T) => string
): FuzzyResult<T>[] {
	const out: FuzzyResult<T>[] = [];
	for (const item of items) {
		const result = score(query, getHaystack(item));
		if (result === null) continue;
		out.push({ item, score: result.score, matchedIndexes: result.matchedIndexes });
	}
	out.sort((a, b) => b.score - a.score);
	return out;
}
