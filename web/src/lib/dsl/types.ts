/** Field widget types used by `<field fieldType="…">` / JSON `fieldType`. */
export type FieldType =
	| 'char'
	| 'text'
	| 'integer'
	| 'float'
	| 'boolean'
	| 'date'
	| 'datetime'
	| 'selection'
	| 'many2one'
	| 'many2many'
	| 'badge'
	| 'currency'
	| 'binary'
	| 'image'
	| 'progress';

export type ViewType = 'list' | 'form' | 'kanban' | 'dashboard' | 'calendar' | 'tree' | 'pivot';

export type ButtonVariant = 'default' | 'outline' | 'secondary' | 'destructive' | 'ghost' | 'link';

export interface PageNodeBase {
	type: string;
	name?: string;
	title?: string;
	label?: string;
	children?: PageNode[];
	[key: string]: unknown;
}

export interface PageNode extends PageNodeBase {
	type: string;
}

export interface PageDef extends PageNode {
	type: 'page';
	route?: string;
	title: string;
}

export interface BuildPageResult {
	page: PageDef;
	warnings: string[];
}
