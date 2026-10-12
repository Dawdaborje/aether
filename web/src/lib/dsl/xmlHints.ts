/**
 * XML tag → JSON `type` mapping for plugin page authors.
 * Pages are authored as `.xml` files and compiled to this JSON shape.
 */
/**
 * Authoring-only tags expanded by the kernel before a page reaches the browser.
 * If one appears in a served tree, `buildPage` will warn (unknown type).
 */
export const reservedXmlTags = ['component', 'use', 'slot', 'set'] as const;

export const xmlTagMap = {
	page: 'page',
	header: 'header',
	footer: 'footer',
	statusbar: 'statusbar',
	notebook: 'notebook',
	group: 'group',
	view: 'view',
	column: 'column',
	columns: 'columns',
	tree: 'tree',
	pivot: 'pivot',
	search: 'search',
	filter: 'filter',
	groupby: 'groupby',
	pager: 'pager',
	pagination: 'pagination',
	breadcrumb: 'breadcrumb',
	menu: 'menu',
	menuitem: 'menuitem',
	divider: 'divider',
	separator: 'separator',
	html: 'html',
	field: 'field',
	label: 'label',
	button: 'button',
	buttonbox: 'buttonbox',
	action: 'action',
	actions: 'actions',
	stat: 'stat',
	kpi: 'kpi',
	chart: 'chart',
	modal: 'modal',
	dialog: 'dialog',
	confirm: 'confirm',
	sheet: 'sheet',
	drawer: 'drawer',
	popover: 'popover',
	tooltip: 'tooltip',
	toast: 'toast',
	notification: 'notification',
	spinner: 'spinner',
	loading: 'loading',
	skeleton: 'skeleton',
	progress: 'progress',
	progressbar: 'progressbar',
	card: 'card',
	templates: 'templates',
	empty: 'empty',
	alert: 'alert',
	banner: 'banner',
	badge: 'badge',
	avatar: 'avatar'
} as const;

export type XmlTag = keyof typeof xmlTagMap;
