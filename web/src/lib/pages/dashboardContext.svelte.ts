import { getContext, setContext } from 'svelte';

const DASH = Symbol('aether.dashboard');

/**
 * Shared payload from a dashboard's `source` function, so child `<stat field="x"/>`
 * widgets can read keys without each calling the plugin.
 */
export class DashboardState {
	data = $state<Record<string, unknown> | null>(null);
	loading = $state(false);
	error = $state<string | null>(null);
}

export function setDashboardState(state: DashboardState): void {
	setContext(DASH, state);
}

export function getDashboardState(): DashboardState | undefined {
	return getContext<DashboardState | undefined>(DASH);
}
