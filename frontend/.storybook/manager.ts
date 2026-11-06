import { addons } from 'storybook/manager-api';
import { create } from 'storybook/theming';

addons.setConfig({
	theme: create({
		base: 'dark',
		appBg: '#0c0f14',
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
	})
});
