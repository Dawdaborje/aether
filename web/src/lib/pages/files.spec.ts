import { describe, expect, it } from 'vitest';
import { actionsFor, ago, iconFor, readListing, type Item } from './files';

const file = (over: Partial<Item> = {}): Item => ({
	id: 'f',
	kind: 'file',
	name: 'a.pdf',
	level: 'owner',
	...over
});

describe('files', () => {
	it('reads a listing and fills the gaps', () => {
		const l = readListing({
			view: 'folder',
			folders: [{ id: 'a', name: 'A' }],
			files: [{ id: 'b', name: 'b.txt' }, null, { name: 'no id' }]
		});
		expect(l.folders[0].kind).toBe('folder');
		expect(l.files.map((f) => f.id)).toEqual(['b']);
		expect(readListing(null)).toMatchObject({
			view: 'mine',
			folder: null,
			files: [],
			can_write: false
		});
	});

	it('picks an icon from the type, then the name', () => {
		expect(iconFor({ kind: 'folder', name: 'x' })).toBe('folder');
		expect(iconFor({ kind: 'file', name: 'x.bin', mime: 'image/png' })).toBe('image');
		expect(iconFor({ kind: 'file', name: 'REPORT.PDF' })).toBe('pdf');
		expect(iconFor({ kind: 'file', name: 'a.xlsx' })).toBe('sheet');
		expect(iconFor({ kind: 'file', name: 'a.zip' })).toBe('archive');
		expect(iconFor({ kind: 'file', name: 'notes.md', mime: 'text/markdown' })).toBe('text');
		expect(iconFor({ kind: 'file', name: 'mystery' })).toBe('file');
	});

	it('offers what the person may do', () => {
		const ids = (item: Item, view = 'folder') => actionsFor(item, view).map((a) => a.id);
		expect(ids(file())).toEqual([
			'open',
			'download',
			'rename',
			'move',
			'favourite',
			'share',
			'comments',
			'versions',
			'trash'
		]);
		expect(ids(file({ level: 'edit' }))).toEqual([
			'open',
			'download',
			'rename',
			'move',
			'favourite',
			'comments',
			'versions',
			'trash'
		]);
		expect(ids(file({ level: 'view' }))).toEqual(['open', 'download', 'favourite', 'comments', 'versions']);
		expect(ids({ id: 'd', kind: 'folder', name: 'D', level: 'owner' })).toEqual([
			'open',
			'rename',
			'move',
			'favourite',
			'share',
			'trash'
		]);
		expect(ids(file(), 'trash')).toEqual(['restore', 'purge']);
	});

	it('offers the office editor for office files, with a label that says whether they may change it', () => {
		const edit = (name: string, level = 'owner') => actionsFor(file({ name, level }), 'folder').find((a) => a.id === 'edit_online');
		expect(edit('Plan.DOCX')?.label).toBe('Edit online');
		expect(edit('sheet.xlsx', 'view')?.label).toBe('Open in editor');
		expect(edit('slides.odp')).toBeDefined();
		expect(edit('a.pdf')).toBeUndefined();
		expect(edit('notes.docx.exe')).toBeUndefined();
	});

	it('says how long ago', () => {
		const now = Date.parse('2026-10-18T12:00:00Z');
		expect(ago('2026-10-18T11:59:50Z', now)).toBe('just now');
		expect(ago('2026-10-18T11:30:00Z', now)).toBe('30 min ago');
		expect(ago('2026-10-18T09:00:00Z', now)).toBe('3 h ago');
		expect(ago('2026-10-16T12:00:00Z', now)).toBe('2 days ago');
		expect(ago(undefined, now)).toBe('');
		expect(ago('nonsense', now)).toBe('');
	});
});
