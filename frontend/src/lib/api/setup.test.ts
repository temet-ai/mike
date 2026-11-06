import { describe, expect, it } from 'vitest';
import { currentPreset, switchesToChange, switchesTurnedOff } from './setup';
import type { FeaturePreset } from './setup';
import type { FeatureDto } from './settings';

const feature = (key: string, enabled: boolean): FeatureDto => ({
	key,
	enabled,
	scope: 'session',
	applies: 'new_sessions'
});

const watchOnly: FeaturePreset = {
	id: 'watch-only',
	toggles: [
		{ key: 'recall_injector', enabled: false },
		{ key: 'steerer', enabled: false }
	],
	lessons_setting: 'reminders.inject_lessons',
	session_lessons: false
};

const recommended: FeaturePreset = {
	id: 'recommended',
	toggles: [
		{ key: 'recall_injector', enabled: true },
		{ key: 'steerer', enabled: false }
	],
	lessons_setting: 'reminders.inject_lessons',
	session_lessons: true
};

const presets: FeaturePreset[] = [watchOnly, recommended];

describe('feature presets', () => {
	it('names the preset whose switches all match', () => {
		const features = [feature('recall_injector', true), feature('steerer', false)];
		expect(currentPreset(presets, features)).toBe('recommended');
	});

	it('returns null when the switches match no preset', () => {
		const features = [feature('recall_injector', true), feature('steerer', true)];
		expect(currentPreset(presets, features)).toBeNull();
	});

	it('lists only the switches that differ and the ones a preset turns off', () => {
		const features = [feature('recall_injector', true), feature('steerer', true)];
		expect(switchesToChange(watchOnly, features).map((t) => t.key)).toEqual([
			'recall_injector',
			'steerer'
		]);
		expect(switchesTurnedOff(watchOnly, features)).toEqual(['recall_injector', 'steerer']);
		expect(switchesToChange(recommended, features).map((t) => t.key)).toEqual(['steerer']);
	});

	it('ignores switches the page does not list', () => {
		expect(switchesToChange(recommended, [feature('steerer', false)])).toEqual([]);
	});
});
