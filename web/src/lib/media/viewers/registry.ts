import type { MediaFile } from '../api';
import type { ViewerDef } from './types';

const viewers = new Map<string, ViewerDef>();

export function registerViewer(def: ViewerDef): void {
	viewers.set(def.id, def);
}

export function unregisterViewer(id: string): void {
	viewers.delete(id);
}

export function allViewers(): ViewerDef[] {
	return [...viewers.values()].sort((a, b) => b.priority - a.priority);
}

/** The viewer that opens a file: the highest priority one that matches. The fallback matches everything. */
export function resolveViewer(file: Pick<MediaFile, 'name' | 'mime'>): ViewerDef | null {
	for (const def of allViewers()) {
		try {
			if (def.matches(file)) return def;
		} catch {
			/* a broken matcher must not stop the others */
		}
	}
	return null;
}
