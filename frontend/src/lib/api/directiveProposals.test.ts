import { describe, expect, it } from 'vitest';
import { sourceLabel } from './directiveProposals';
import { candidateRows } from './crossSession';

describe('directive proposal helpers', () => {
	it('names each source in the operator’s terms', () => {
		expect(sourceLabel('repeated_directive')).toBe('You said this more than once');
		expect(sourceLabel('recurring_pattern')).toBe('recurring pattern');
	});

	it('keeps only the all-bins candidate rows', () => {
		const rows = candidateRows({
			snapshot: {
				settings: { input_mode: 'off', threshold_mode: 'off', calibration_min_labelled: 20 },
				rollup: null,
				rows: [
					{
						family: 'steer_calibration',
						question: 'relapse',
						scope_kind: 'global',
						dimension: 'all',
						dimension_value: 'all',
						k: 1,
						n: 2,
						ci_low: null,
						ci_high: null
					},
					{
						family: 'steer_calibration',
						question: 'relapse',
						scope_kind: 'global',
						dimension: 'confidence_bin_05',
						dimension_value: '0.90',
						k: 1,
						n: 1,
						ci_low: null,
						ci_high: null
					}
				],
				thresholds: []
			},
			block_if_live: null
		});
		expect(rows).toHaveLength(1);
	});
});
