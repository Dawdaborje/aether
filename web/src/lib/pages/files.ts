/** The logic of the `<files>` browser, apart from the drawing. */
export interface Item {
	id: string;
	kind: 'file' | 'folder';
	name: string;
	owner_name?: string;
	modified_at?: string;
	size?: number;
	mime?: string;
	version?: number;
	level?: string;
	shared?: boolean;
	favourite?: boolean;
	/** In a search: `name` or `text` (the words are inside the file). */
	found_in?: string;
	trashed?: boolean;
	modified_by?: string;
	media?: { key: string; name?: string; mime?: string; size?: number } | null;
}

export interface Listing {
	view: string;
	folder: { id: string; name: string; level: string } | null;
	path: { id: string; name: string }[];
	can_write: boolean;
	folders: Item[];
	files: Item[];
	/** What the organization has switched on; both on when the function does not say. */
	features: { links: boolean; comments: boolean; settings: boolean };
}

/** What a browse function answers, with anything missing filled in so the widget can draw it. */
export function readListing(data: unknown): Listing {
	const d = (data && typeof data === 'object' ? data : {}) as Partial<Listing>;
	const items = (list: unknown, kind: 'file' | 'folder'): Item[] =>
		Array.isArray(list)
			? (list as Item[]).filter((i) => i && typeof i.id === 'string').map((i) => ({ ...i, kind }))
			: [];
	return {
		view: typeof d.view === 'string' ? d.view : 'mine',
		folder: d.folder && typeof d.folder === 'object' ? d.folder : null,
		path: Array.isArray(d.path) ? d.path : [],
		can_write: d.can_write === true,
		folders: items(d.folders, 'folder'),
		files: items(d.files, 'file'),
		features: {
			links: d.features?.links !== false,
			comments: d.features?.comments !== false,
			settings: d.features?.settings === true
		}
	};
}

export type IconName =
	| 'folder'
	| 'image'
	| 'pdf'
	| 'sheet'
	| 'slides'
	| 'doc'
	| 'archive'
	| 'audio'
	| 'video'
	| 'text'
	| 'file';

/** Which icon a file gets, from its type and name. */
export function iconFor(item: Pick<Item, 'kind' | 'mime' | 'name'>): IconName {
	if (item.kind === 'folder') return 'folder';
	const mime = (item.mime ?? '').toLowerCase();
	const ext = item.name.toLowerCase().split('.').pop() ?? '';
	if (mime.startsWith('image/')) return 'image';
	if (mime.startsWith('audio/')) return 'audio';
	if (mime.startsWith('video/')) return 'video';
	if (mime === 'application/pdf' || ext === 'pdf') return 'pdf';
	if (['xlsx', 'xls', 'ods', 'csv'].includes(ext)) return 'sheet';
	if (['pptx', 'ppt', 'odp'].includes(ext)) return 'slides';
	if (['docx', 'doc', 'odt'].includes(ext)) return 'doc';
	if (['zip', 'gz', 'tar', '7z'].includes(ext) || mime === 'application/zip') return 'archive';
	if (mime.startsWith('text/') || ['txt', 'md', 'log', 'json', 'xml'].includes(ext)) return 'text';
	return 'file';
}

export interface Action {
	id:
		| 'open'
		| 'download'
		| 'edit_online'
		| 'rename'
		| 'move'
		| 'share'
		| 'versions'
		| 'comments'
		| 'favourite'
		| 'trash'
		| 'restore'
		| 'purge';
	label: string;
}

/** Files an office editor opens (by the end of the name). */
export const EDITABLE = /\.(docx?|xlsx?|pptx?|odt|ods|odp|rtf|csv)$/i;

/** The actions a row offers, from its level and where the listing is. `level` is `view`, `edit` or `owner`. */
export function actionsFor(item: Item, view: string): Action[] {
	if (view === 'trash') {
		return [
			{ id: 'restore', label: 'Restore' },
			{ id: 'purge', label: 'Delete forever' }
		];
	}
	const level = item.level ?? 'view';
	const edit = level === 'edit' || level === 'owner';
	const out: Action[] = [{ id: 'open', label: 'Open' }];
	if (item.kind === 'file') out.push({ id: 'download', label: 'Download' });
	if (item.kind === 'file' && EDITABLE.test(item.name)) out.push({ id: 'edit_online', label: edit ? 'Edit online' : 'Open in editor' });
	if (edit) {
		out.push({ id: 'rename', label: 'Rename' }, { id: 'move', label: 'Move to…' });
	}
	out.push({
		id: 'favourite',
		label: item.favourite ? 'Remove from favourites' : 'Add to favourites'
	});
	if (level === 'owner') out.push({ id: 'share', label: 'Share…' });
	if (item.kind === 'file') out.push({ id: 'comments', label: 'Comments' });
	if (item.kind === 'file') out.push({ id: 'versions', label: 'Versions' });
	if (edit) out.push({ id: 'trash', label: 'Move to trash' });
	return out;
}

/** A short "3 days ago" style time, from an ISO date; blank when there is none. */
export function ago(iso: string | undefined, now = Date.now()): string {
	const t = iso ? Date.parse(iso) : NaN;
	if (Number.isNaN(t)) return '';
	const s = Math.max(0, Math.round((now - t) / 1000));
	if (s < 60) return 'just now';
	const m = Math.round(s / 60);
	if (m < 60) return `${m} min ago`;
	const h = Math.round(m / 60);
	if (h < 24) return `${h} h ago`;
	const d = Math.round(h / 24);
	if (d < 30) return `${d} day${d === 1 ? '' : 's'} ago`;
	return new Date(t).toLocaleDateString();
}
