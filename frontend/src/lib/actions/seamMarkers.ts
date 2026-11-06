const TOLERANCE_PX = 1;
const MARKER_LONG_PX = 42;
const MARKER_SHORT_PX = 27;
const MARKER_BORDER_PX = 1.5;

type SeamAxis = 'horizontal-seam' | 'vertical-seam';

interface SeamTouch {
	a: HTMLElement;
	b: HTMLElement;
	axis: SeamAxis;
	centerX: number;
	centerY: number;
}

const HEX_ALONG_X = 'polygon(25% 0%, 75% 0%, 100% 50%, 75% 100%, 25% 100%, 0% 50%)';
const HEX_ALONG_Y = 'polygon(0% 25%, 0% 75%, 50% 100%, 100% 75%, 100% 25%, 50% 0%)';

let uidCounter = 0;
const elementIds = new WeakMap<HTMLElement, string>();

function idFor(el: HTMLElement): string {
	let id = elementIds.get(el);
	if (!id) {
		id = `seam-el-${uidCounter++}`;
		elementIds.set(el, id);
	}
	return id;
}

function pairKey(a: HTMLElement, b: HTMLElement): string {
	return `${idFor(a)}::${idFor(b)}`;
}

function overlapRun(
	aStart: number,
	aEnd: number,
	bStart: number,
	bEnd: number
): [number, number] | null {
	const start = Math.max(aStart, bStart);
	const end = Math.min(aEnd, bEnd);
	return end - start > TOLERANCE_PX ? [start, end] : null;
}

function detectTouches(children: HTMLElement[]): SeamTouch[] {
	const rects = children.map((el) => el.getBoundingClientRect());
	const touches: SeamTouch[] = [];

	for (let i = 0; i < children.length; i++) {
		for (let j = i + 1; j < children.length; j++) {
			const r1 = rects[i]!;
			const r2 = rects[j]!;

			const vOverlap = overlapRun(r1.left, r1.right, r2.left, r2.right);
			if (vOverlap) {
				if (Math.abs(r1.bottom - r2.top) < TOLERANCE_PX) {
					touches.push({
						a: children[i]!,
						b: children[j]!,
						axis: 'horizontal-seam',
						centerX: (vOverlap[0] + vOverlap[1]) / 2,
						centerY: r1.bottom
					});
					continue;
				}
				if (Math.abs(r2.bottom - r1.top) < TOLERANCE_PX) {
					touches.push({
						a: children[j]!,
						b: children[i]!,
						axis: 'horizontal-seam',
						centerX: (vOverlap[0] + vOverlap[1]) / 2,
						centerY: r2.bottom
					});
					continue;
				}
			}

			const hOverlap = overlapRun(r1.top, r1.bottom, r2.top, r2.bottom);
			if (hOverlap) {
				if (Math.abs(r1.right - r2.left) < TOLERANCE_PX) {
					touches.push({
						a: children[i]!,
						b: children[j]!,
						axis: 'vertical-seam',
						centerX: r1.right,
						centerY: (hOverlap[0] + hOverlap[1]) / 2
					});
					continue;
				}
				if (Math.abs(r2.right - r1.left) < TOLERANCE_PX) {
					touches.push({
						a: children[j]!,
						b: children[i]!,
						axis: 'vertical-seam',
						centerX: r2.right,
						centerY: (hOverlap[0] + hOverlap[1]) / 2
					});
				}
			}
		}
	}

	return touches;
}

function readAccent(el: HTMLElement, prop: string, fallback: string): string {
	const v = getComputedStyle(el).getPropertyValue(prop).trim();
	return v || fallback;
}

function buildMarker(): HTMLElement {
	const outer = document.createElement('div');
	outer.className = 'seam-marker';
	outer.dataset.slot = 'seam-marker';
	outer.setAttribute('aria-hidden', 'true');
	outer.style.position = 'absolute';
	outer.style.pointerEvents = 'none';

	const inner = document.createElement('div');
	inner.className = 'seam-marker-inner';
	inner.style.position = 'absolute';
	inner.style.inset = `${MARKER_BORDER_PX}px`;
	outer.appendChild(inner);

	return outer;
}

function updateMarker(marker: HTMLElement, touch: SeamTouch, containerRect: DOMRect): void {
	const inner = marker.firstElementChild as HTMLElement | null;
	const isHorizontalSeam = touch.axis === 'horizontal-seam';
	const width = isHorizontalSeam ? MARKER_LONG_PX : MARKER_SHORT_PX;
	const height = isHorizontalSeam ? MARKER_SHORT_PX : MARKER_LONG_PX;
	const clipPath = isHorizontalSeam ? HEX_ALONG_X : HEX_ALONG_Y;
	const gradientAngle = isHorizontalSeam ? '180deg' : '90deg';

	marker.dataset.seamAxis = touch.axis;
	marker.style.width = `${width}px`;
	marker.style.height = `${height}px`;
	marker.style.left = `${touch.centerX - containerRect.left - width / 2}px`;
	marker.style.top = `${touch.centerY - containerRect.top - height / 2}px`;
	marker.style.clipPath = clipPath;

	const aBorder = readAccent(touch.a, '--acc-border', 'var(--border-default)');
	const bBorder = readAccent(touch.b, '--acc-border', 'var(--border-default)');
	const aFg = readAccent(touch.a, '--acc-fg', 'var(--text-3)');
	const bFg = readAccent(touch.b, '--acc-fg', 'var(--text-3)');

	marker.style.background = `linear-gradient(${gradientAngle}, ${aBorder} 0%, ${bBorder} 100%)`;

	if (inner) {
		inner.style.clipPath = clipPath;
		inner.style.background = `linear-gradient(${gradientAngle}, ${aFg} 0%, ${bFg} 100%)`;
	}
}

export function seamMarkersAction(container: HTMLElement): { destroy: () => void } {
	if (getComputedStyle(container).position === 'static') {
		container.style.position = 'relative';
	}

	const markers = new Map<string, HTMLElement>();

	function measure(): void {
		const children = Array.from(container.children).filter(
			(el): el is HTMLElement => el instanceof HTMLElement && el.dataset.slot !== 'seam-marker'
		);
		const touches = detectTouches(children);
		const containerRect = container.getBoundingClientRect();
		const seen = new Set<string>();

		for (const touch of touches) {
			const key = pairKey(touch.a, touch.b);
			seen.add(key);
			let marker = markers.get(key);
			if (!marker) {
				marker = buildMarker();
				markers.set(key, marker);
				container.appendChild(marker);
			}
			updateMarker(marker, touch, containerRect);
		}

		for (const [key, marker] of markers) {
			if (!seen.has(key)) {
				marker.remove();
				markers.delete(key);
			}
		}
	}

	const ro = new ResizeObserver(measure);
	ro.observe(container);
	measure();

	return {
		destroy(): void {
			ro.disconnect();
			for (const marker of markers.values()) marker.remove();
			markers.clear();
		}
	};
}
