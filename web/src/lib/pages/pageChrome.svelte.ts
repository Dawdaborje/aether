import { getContext, setContext } from 'svelte';

const CHROME = Symbol('aether.pageChrome');

/**
 * Odoo-style control panel chrome for a plugin page: New under the breadcrumbs,
 * search on the right. The primary list (or later kanban) registers into this;
 * PageShell reads it to fill ControlPanel.
 */
export class PageChrome {
	createHref = $state<string | null>(null);
	createLabel = $state('New');
	searchable = $state(false);
	query = $state('');

	clear(): void {
		this.createHref = null;
		this.createLabel = 'New';
		this.searchable = false;
		this.query = '';
	}
}

export function setPageChrome(chrome: PageChrome): void {
	setContext(CHROME, chrome);
}

export function getPageChrome(): PageChrome | undefined {
	return getContext<PageChrome | undefined>(CHROME);
}
