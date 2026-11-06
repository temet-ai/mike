import { describe, expect, it } from 'vitest';
import { timelineTrackWidth } from './FlowPlayerBand.svelte';
import { buildMarkers } from '$lib/session-flow/markers';
import { flowTimeToX } from '$lib/session-flow/timeline';
import { fleetDataset } from '$lib/session-flow/__fixtures__/fleet';

describe('FlowPlayerBand timeline track geometry', () => {
	it('derives one track width every sub-part builds its x-mapping against', () => {
		expect(timelineTrackWidth(960)).toBe(960 - 88 - 8);
		expect(timelineTrackWidth(10)).toBe(1);
	});

	it('lines up a marker (buildMarkers) with flowTimeToX on the same track width', () => {
		const dataset = fleetDataset();
		const bounds = dataset.bounds!;
		const bandWidth = 900;
		const track = timelineTrackWidth(bandWidth);

		const markers = buildMarkers(dataset, track);
		const evidenceStart = markers.find((marker) => marker.id === 'evidence-start');
		expect(evidenceStart).toBeDefined();

		expect(evidenceStart!.x).toBe(flowTimeToX(bounds.startMs, bounds, track));
		expect(evidenceStart!.atMs).toBe(bounds.startMs);
	});
});
