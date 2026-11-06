import type { Preview } from '@storybook/sveltekit';
import { create } from 'storybook/theming';
import { withThemeByDataAttribute } from '@storybook/addon-themes';
import { THEMES as THEME_REGISTRY } from '../src/lib/themes';
import '@fontsource-variable/jetbrains-mono/index.css';
import '@fontsource-variable/jetbrains-mono/wght-italic.css';
import '@fontsource-variable/syne/index.css';
import '@fontsource-variable/inter/index.css';
import '@fontsource-variable/geist-mono/index.css';
import 'augmented-ui/augmented-ui.min.css';
import '../src/app.css';
import { configureCacheFromSettings } from '../src/lib/stores/cache/cacheConfig';
import { __resetApiCacheForTests } from '../src/lib/stores/cache/apiCache.svelte';
import { __resetCacheForTests } from '../src/lib/stores/cache/unifiedCache.svelte';

const THEMES = Object.fromEntries(THEME_REGISTRY.map((t) => [t.id, t.id])) as Record<
	(typeof THEME_REGISTRY)[number]['id'],
	(typeof THEME_REGISTRY)[number]['id']
>;

const THEME_OVERRIDE = (import.meta.env?.STORYBOOK_THEME as string | undefined) || undefined;
const DEFAULT_THEME = THEME_OVERRIDE && THEME_OVERRIDE in THEMES ? THEME_OVERRIDE : 'forge';

const DISABLE_MOTION = import.meta.env?.STORYBOOK_DISABLE_MOTION === 'true';

const darkTheme = create({
	base: 'dark',
	appBg: '#0f1117',
	appContentBg: '#0f1117',
	appPreviewBg: '#0f1117',
	appBorderColor: '#2a2d35',
	appBorderRadius: 4,
	fontBase: '"Inter Variable", "Inter", system-ui, sans-serif',
	fontCode: '"JetBrains Mono Variable", "JetBrains Mono", monospace',
	textColor: '#e2e4eb',
	textMutedColor: '#8b8fa8',
	barBg: '#0c0f14',
	barTextColor: '#8b8fa8',
	barSelectedColor: '#7eb8f7',
	barHoverColor: '#e2e4eb',
	colorPrimary: '#7eb8f7',
	colorSecondary: '#7eb8f7'
});

configureCacheFromSettings({ enabled: false });

if (
	DISABLE_MOTION &&
	typeof Element !== 'undefined' &&
	typeof Element.prototype.animate === 'function'
) {
	const nativeAnimate = Element.prototype.animate;
	Element.prototype.animate = function (keyframes, options) {
		const animation = nativeAnimate.call(this, keyframes, options);
		queueMicrotask(() => {
			try {
				animation.finish();
			} catch {}
		});
		return animation;
	};
}

if (
	typeof location !== 'undefined' &&
	new URLSearchParams(location.search).has('aug-transparent')
) {
	const style = document.createElement('style');
	style.textContent = 'html,body,#storybook-root{background:transparent !important;margin:0}';
	document.head.appendChild(style);
}

const APP_FACES = [
	'400 1em "JetBrains Mono Variable"',
	'600 1em "JetBrains Mono Variable"',
	'600 1em "Syne Variable"',
	'700 1em "Syne Variable"',
	'400 1em "Inter Variable"'
];

async function facesLoaded(): Promise<void> {
	if (typeof document === 'undefined' || !document.fonts) return;
	await Promise.all(APP_FACES.map((face) => document.fonts.load(face)));
	await document.fonts.ready;
}

const preview: Preview = {
	async beforeEach() {
		await facesLoaded();
		configureCacheFromSettings({ enabled: false });
		__resetApiCacheForTests();
		__resetCacheForTests();
	},
	initialGlobals: {
		theme: DEFAULT_THEME
	},
	parameters: {
		controls: {
			matchers: {
				color: /(background|color)$/i,
				date: /Date$/i
			}
		},
		a11y: {
			test: 'error',
			options: {
				rules: {
					'color-contrast': { enabled: false },
					'color-contrast-enhanced': { enabled: false }
				}
			}
		},
		docs: {
			theme: darkTheme,
			source: { type: 'code' }
		}
	},
	decorators: [
		withThemeByDataAttribute({
			themes: THEMES,
			defaultTheme: DEFAULT_THEME,
			attributeName: 'data-theme',
			parentSelector: 'html'
		}),
		(story, context) => {
			if (typeof document !== 'undefined') {
				const toolbarTheme = context.globals?.theme as string | undefined;
				const active =
					(THEME_OVERRIDE && THEME_OVERRIDE in THEMES && THEME_OVERRIDE) ||
					(toolbarTheme && toolbarTheme in THEMES && toolbarTheme) ||
					DEFAULT_THEME;
				document.documentElement.setAttribute('data-theme', active);
				document.documentElement.style.background = 'var(--surface-bg)';
				document.body.style.background = 'var(--surface-bg)';
				document.body.style.color = 'var(--text-1)';
			}
			return story();
		}
	]
};

export default preview;
