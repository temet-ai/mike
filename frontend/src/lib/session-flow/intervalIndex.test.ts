import { describe, expect, it } from 'vitest';
import {
	FLOW_START_MS as start,
	flowActivity,
	flowDataset,
	flowTime
} from './__fixtures__/sessionFlow';
import { entriesThrough, openCaptureIdsAt } from './intervalIndex';

describe('session-flow interval index', () => {
	it('uses half-open bounds and cumulative zero-duration points', () => {
		const { index } = flowDataset([
			flowActivity('interval', { ended_at: flowTime(1000) }),
			flowActivity('point', { ended_at: flowTime(0) })
		]);
		expect(openCaptureIdsAt(index, start - 1).size).toBe(0);
		expect(openCaptureIdsAt(index, start)).toEqual(new Set(['interval']));
		expect(openCaptureIdsAt(index, start + 1000).size).toBe(0);
		expect(entriesThrough(index.points, start + 10000)).toEqual([{ id: 'point', atMs: start }]);
	});
	it('sorts tied phases by full stable ID regardless of input order', () => {
		const first = flowDataset([flowActivity('z'), flowActivity('a')]);
		const second = flowDataset([flowActivity('a'), flowActivity('z')]);
		expect(first.index).toEqual(second.index);
		expect(entriesThrough(first.index.phases, start).map(({ id }) => id)).toEqual(['a', 'z']);
	});
	it('does not turn measured latency into an absolute span', () => {
		const activity = flowActivity('llm', {
			time_semantics: 'measured_latency',
			ended_at: flowTime(5000)
		});
		activity.metadata.latency_ms = 5000;
		const { index } = flowDataset([activity]);
		expect(index.starts).toEqual([]);
		expect(index.ends).toEqual([]);
		expect(index.points).toHaveLength(1);
	});
});
