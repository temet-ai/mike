/** Splits a comma-separated field into trimmed, non-empty names. */
export function splitList(text: string): string[] {
	return text
		.split(',')
		.map((part) => part.trim())
		.filter((part) => part !== '');
}

export function joinList(items: readonly string[]): string {
	return items.join(', ');
}
