import { slide, type TransitionConfig } from 'svelte/transition';

export function prefersLessMotion(): boolean {
	if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return false;
	return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

function tokenValue(node: Element, token: string): string {
	const own = getComputedStyle(node).getPropertyValue(token).trim();
	if (own) return own;
	return getComputedStyle(document.documentElement).getPropertyValue(token).trim();
}

export function motionDuration(node: Element, token: string): number {
	if (prefersLessMotion()) return 0;
	const raw = tokenValue(node, token);
	if (!raw) return 0;
	const value = Number.parseFloat(raw);
	if (!Number.isFinite(value)) return 0;
	return raw.endsWith('ms') ? value : value * 1000;
}

export function motionEasing(node: Element, token: string): (t: number) => number {
	const raw = tokenValue(node, token);
	const match = raw.match(/^cubic-bezier\(([^)]+)\)$/);
	if (!match?.[1]) return (t) => t;
	const points = match[1].split(',').map((part) => Number.parseFloat(part));
	if (points.length !== 4 || points.some((p) => !Number.isFinite(p))) return (t) => t;
	const [x1, y1, x2, y2] = points as [number, number, number, number];
	return (t: number) => cubicBezier(x1, y1, x2, y2, t);
}

function axis(a: number, b: number, t: number): number {
	const u = 1 - t;
	return 3 * u * u * t * a + 3 * u * t * t * b + t * t * t;
}

function cubicBezier(x1: number, y1: number, x2: number, y2: number, x: number): number {
	if (x <= 0) return 0;
	if (x >= 1) return 1;
	let low = 0;
	let high = 1;
	let t = x;
	for (let i = 0; i < 24; i += 1) {
		if (axis(x1, x2, t) < x) low = t;
		else high = t;
		t = (low + high) / 2;
	}
	return axis(y1, y2, t);
}

export function disclose(node: Element): TransitionConfig {
	return {
		duration: motionDuration(node, '--dur-2'),
		easing: motionEasing(node, '--ease-snap'),
		css: (t: number) => `grid-template-rows: ${t}fr; opacity: ${t};`
	};
}

export function discloseHeight(node: Element): TransitionConfig {
	return slide(node, {
		duration: motionDuration(node, '--dur-2'),
		easing: motionEasing(node, '--ease-snap')
	});
}
