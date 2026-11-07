import { viewKind, type ViewKind } from '../api';
import ArchiveViewer from './ArchiveViewer.svelte';
import FallbackViewer from './FallbackViewer.svelte';
import ImageViewer from './ImageViewer.svelte';
import EmlViewer from './EmlViewer.svelte';
import HtmlViewer from './HtmlViewer.svelte';
import MarkdownViewer from './MarkdownViewer.svelte';
import OfficeViewer from './OfficeViewer.svelte';
import PdfViewer from './PdfViewer.svelte';
import PlayerViewer from './PlayerViewer.svelte';
import TableViewer from './TableViewer.svelte';
import TextViewer from './TextViewer.svelte';
import { registerViewer } from './registry';
import type { ViewerDef } from './types';

const is =
	(...kinds: ViewKind[]) =>
	(file: Parameters<typeof viewKind>[0]) =>
		kinds.includes(viewKind(file));

const builtins: ViewerDef[] = [
	{
		id: 'image',
		label: 'Image',
		priority: 100,
		matches: is('image'),
		component: ImageViewer,
		canExtract: true
	},
	{
		id: 'pdf',
		label: 'PDF',
		priority: 100,
		matches: is('pdf'),
		component: PdfViewer,
		canExtract: true
	},
	{
		id: 'office',
		label: 'Office document',
		priority: 100,
		matches: is('office'),
		component: OfficeViewer,
		canExtract: true
	},
	{ id: 'table', label: 'Table', priority: 100, matches: is('csv'), component: TableViewer },
	{
		id: 'markdown',
		label: 'Markdown',
		priority: 100,
		matches: is('markdown'),
		component: MarkdownViewer
	},
	{ id: 'html', label: 'HTML', priority: 100, matches: is('html'), component: HtmlViewer },
	{ id: 'email', label: 'Email', priority: 100, matches: is('email'), component: EmlViewer },
	{ id: 'text', label: 'Text', priority: 100, matches: is('text', 'json'), component: TextViewer },
	{
		id: 'player',
		label: 'Audio and video',
		priority: 100,
		matches: is('audio', 'video'),
		component: PlayerViewer
	},
	{
		id: 'archive',
		label: 'Archive',
		priority: 100,
		matches: is('archive'),
		component: ArchiveViewer
	},
	{
		id: 'fallback',
		label: 'Download',
		priority: -1000,
		matches: () => true,
		component: FallbackViewer
	}
];

let done = false;

/** Registers the viewers that ship with the app. Safe to call more than once. */
export function registerBuiltinViewers(): void {
	if (done) return;
	done = true;
	for (const def of builtins) registerViewer(def);
}
