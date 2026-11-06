export function pluralRequests(count: number): string {
	return `${count} request${count === 1 ? '' : 's'}`;
}
