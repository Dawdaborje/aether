import { getContext, setContext } from 'svelte';
import type { OrganizationProfile } from './api';

const KEY = 'aether.org.detail';

export interface OrgDetailContext {
	get organization(): OrganizationProfile | null;
	reload(): Promise<void>;
	setOrganization(next: OrganizationProfile): void;
}

export function setOrgDetailContext(ctx: OrgDetailContext) {
	setContext(KEY, ctx);
}

export function getOrgDetailContext(): OrgDetailContext {
	return getContext(KEY);
}
