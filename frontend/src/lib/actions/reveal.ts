import { prefersLessMotion } from '$lib/components/ui/motion';

export function revealSections(node: HTMLElement, options: { selector?: string } = {}) {
	if (typeof IntersectionObserver === 'undefined' || typeof node.animate !== 'function') return;
	if (prefersLessMotion()) return;
	const selector = options.selector ?? '[data-reveal]';
	const seen = new WeakSet<Element>();
	const animations = new Map<Element, Animation>();
	const style = getComputedStyle(node);
	const duration = Number.parseFloat(style.getPropertyValue('--dur-3')) || 480;
	const stagger = (Number.parseFloat(style.getPropertyValue('--dur-1')) || 120) / 3;
	const distance = style.getPropertyValue('--space-3').trim() || '12px';
	const easing = style.getPropertyValue('--ease-snap').trim() || 'ease-out';

	function reveal(element: Element, index: number): void {
		if (element.contains(document.activeElement)) return;
		const animation = element.animate(
			[
				{ opacity: 0, translate: `0 ${distance}` },
				{ opacity: 1, translate: '0 0' }
			],
			{ duration, delay: Math.min(index, 3) * stagger, easing, fill: 'backwards' }
		);
		animation.id = 'funk-section-reveal';
		animations.set(element, animation);
		const forget = () => animations.delete(element);
		void animation.finished.then(forget, forget);
	}

	const observer = new IntersectionObserver(
		(entries) => {
			entries
				.filter((entry) => entry.isIntersecting)
				.forEach((entry, index) => {
					observer.unobserve(entry.target);
					reveal(entry.target, index);
				});
		},
		{ threshold: 0, rootMargin: '0px 0px -24px 0px' }
	);

	function observe(element: Element): void {
		if (seen.has(element)) return;
		seen.add(element);
		observer.observe(element);
	}
	function discover(root: Element): void {
		if (root.matches(selector)) observe(root);
		root.querySelectorAll(selector).forEach(observe);
	}
	discover(node);
	const changes = new MutationObserver((records) => {
		for (const record of records) {
			for (const added of record.addedNodes) {
				if (added instanceof Element) discover(added);
			}
			for (const removed of record.removedNodes) {
				if (!(removed instanceof Element)) continue;
				for (const element of [removed, ...removed.querySelectorAll(selector)]) {
					observer.unobserve(element);
					animations.get(element)?.cancel();
				}
			}
		}
	});
	changes.observe(node, { childList: true, subtree: true });

	function settle(): void {
		for (const animation of animations.values()) animation.cancel();
		animations.clear();
	}
	function onFocus(event: FocusEvent): void {
		for (const [element, animation] of animations) {
			if (event.target instanceof Node && element.contains(event.target)) animation.cancel();
		}
	}
	node.addEventListener('focusin', onFocus);
	return {
		destroy() {
			observer.disconnect();
			changes.disconnect();
			node.removeEventListener('focusin', onFocus);
			settle();
		}
	};
}
