export function isAtBottom(
	scrollTop: number,
	scrollHeight: number,
	clientHeight: number,
	threshold = 150
): boolean {
	if (scrollHeight <= clientHeight) return true;
	return scrollHeight - scrollTop - clientHeight <= threshold;
}
