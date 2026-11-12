import configurationAndImport from './content/configuration-and-import.md?raw';
import installedSetup from './content/installed-setup.md?raw';
import connectionsAndPrivacy from './content/connections-and-privacy.md?raw';
import recoveryAndMaintenance from './content/recovery-and-maintenance.md?raw';
import threads from './content/threads.md?raw';

/** Explicit imports keep this selected user documentation inside the packaged SPA. */
export const DOCUMENT_CONTENT: Readonly<Record<string, string>> = {
	'installed-setup': installedSetup,
	'configuration-and-import': configurationAndImport,
	'connections-and-privacy': connectionsAndPrivacy,
	'recovery-and-maintenance': recoveryAndMaintenance,
	threads
};
