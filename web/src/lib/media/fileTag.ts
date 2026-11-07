import { viewKind, type ViewKind } from './api';

const LABELS: Record<ViewKind, [string, string]> = {
	image: ['IMG', 'bg-sky-600'],
	pdf: ['PDF', 'bg-red-600'],
	csv: ['CSV', 'bg-emerald-600'],
	json: ['JSON', 'bg-amber-600'],
	text: ['TXT', 'bg-slate-500'],
	markdown: ['MD', 'bg-slate-700'],
	html: ['HTML', 'bg-orange-600'],
	email: ['EML', 'bg-indigo-600'],
	office: ['DOC', 'bg-blue-700'],
	audio: ['AUDIO', 'bg-fuchsia-600'],
	video: ['VIDEO', 'bg-purple-600'],
	archive: ['ZIP', 'bg-yellow-700'],
	other: ['FILE', 'bg-slate-500']
};

/** The short label and background colour of the tile that stands for a file: its kind, or its extension if unknown. */
export function fileTag(file: { name: string; mime?: string }): { label: string; colour: string } {
	const kind = viewKind({ name: file.name, mime: file.mime ?? '' });
	const [label, colour] = LABELS[kind];
	const ext = file.name.includes('.') ? (file.name.split('.').pop() ?? '') : '';
	return { label: kind === 'other' && ext ? ext.slice(0, 5).toUpperCase() : label, colour };
}
