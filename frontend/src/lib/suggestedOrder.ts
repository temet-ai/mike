export type ListOrder = 'default' | 'suggested';

export type AcceptChances = Record<string, number>;

export const ORDER_PARAM = 'sort';

export function orderFromParam(value: string | null): ListOrder {
	return value === 'suggested' ? 'suggested' : 'default';
}

export function sortQuery(order: ListOrder): string {
	return order === 'suggested' ? 'sort=suggested' : '';
}

export function chanceFor(chances: AcceptChances | undefined, id: number | null): number | null {
	if (chances === undefined || id === null) return null;
	const value = chances[String(id)];
	return typeof value === 'number' ? value : null;
}

export function chanceText(chance: number | null): string {
	return chance === null ? '—' : `${Math.round(chance * 100)}%`;
}
