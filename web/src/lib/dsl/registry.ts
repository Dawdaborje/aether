import type { Component } from 'svelte';
import PageShell from '$lib/components/widgets/PageShell.svelte';
import HeaderWidget from '$lib/components/widgets/HeaderWidget.svelte';
import NotebookWidget from '$lib/components/widgets/NotebookWidget.svelte';
import GroupWidget from '$lib/components/widgets/GroupWidget.svelte';
import ListViewWidget from '$lib/components/widgets/ListViewWidget.svelte';
import FormViewWidget from '$lib/components/widgets/FormViewWidget.svelte';
import KanbanViewWidget from '$lib/components/widgets/KanbanViewWidget.svelte';
import DashboardViewWidget from '$lib/components/widgets/DashboardViewWidget.svelte';
import FieldWidget from '$lib/components/widgets/FieldWidget.svelte';
import ButtonWidget from '$lib/components/widgets/ButtonWidget.svelte';
import ActionsWidget from '$lib/components/widgets/ActionsWidget.svelte';
import SearchWidget from '$lib/components/widgets/SearchWidget.svelte';
import ModalWidget from '$lib/components/widgets/ModalWidget.svelte';
import ConfirmWidget from '$lib/components/widgets/ConfirmWidget.svelte';
import SpinnerWidget from '$lib/components/widgets/SpinnerWidget.svelte';
import EmptyWidget from '$lib/components/widgets/EmptyWidget.svelte';
import AlertWidget from '$lib/components/widgets/AlertWidget.svelte';
import StatWidget from '$lib/components/widgets/StatWidget.svelte';
import SeparatorWidget from '$lib/components/widgets/SeparatorWidget.svelte';
import ToastWidget from '$lib/components/widgets/ToastWidget.svelte';
import SkeletonWidget from '$lib/components/widgets/SkeletonWidget.svelte';
import ChartWidget from '$lib/components/widgets/ChartWidget.svelte';
import FallbackWidget from '$lib/components/widgets/FallbackWidget.svelte';
import ChildrenPassthrough from '$lib/components/widgets/ChildrenPassthrough.svelte';
import ViewRouter from '$lib/components/widgets/ViewRouter.svelte';

export type WidgetComponent = Component<Record<string, unknown>>;

export const widgetRegistry: Record<string, WidgetComponent> = {
	page: PageShell as WidgetComponent,
	header: HeaderWidget as WidgetComponent,
	footer: HeaderWidget as WidgetComponent,
	statusbar: HeaderWidget as WidgetComponent,
	notebook: NotebookWidget as WidgetComponent,
	group: GroupWidget as WidgetComponent,
	view: ViewRouter as WidgetComponent,
	list: ListViewWidget as WidgetComponent,
	form: FormViewWidget as WidgetComponent,
	kanban: KanbanViewWidget as WidgetComponent,
	dashboard: DashboardViewWidget as WidgetComponent,
	columns: ChildrenPassthrough as WidgetComponent,
	column: ChildrenPassthrough as WidgetComponent,
	field: FieldWidget as WidgetComponent,
	label: FieldWidget as WidgetComponent,
	button: ButtonWidget as WidgetComponent,
	buttonbox: ActionsWidget as WidgetComponent,
	action: ButtonWidget as WidgetComponent,
	actions: ActionsWidget as WidgetComponent,
	search: SearchWidget as WidgetComponent,
	filter: SearchWidget as WidgetComponent,
	modal: ModalWidget as WidgetComponent,
	dialog: ModalWidget as WidgetComponent,
	confirm: ConfirmWidget as WidgetComponent,
	spinner: SpinnerWidget as WidgetComponent,
	loading: SpinnerWidget as WidgetComponent,
	skeleton: SkeletonWidget as WidgetComponent,
	empty: EmptyWidget as WidgetComponent,
	alert: AlertWidget as WidgetComponent,
	banner: AlertWidget as WidgetComponent,
	stat: StatWidget as WidgetComponent,
	kpi: StatWidget as WidgetComponent,
	separator: SeparatorWidget as WidgetComponent,
	divider: SeparatorWidget as WidgetComponent,
	toast: ToastWidget as WidgetComponent,
	notification: ToastWidget as WidgetComponent,
	chart: ChartWidget as WidgetComponent,
	sheet: ModalWidget as WidgetComponent,
	drawer: ModalWidget as WidgetComponent,
	tree: ListViewWidget as WidgetComponent,
	pivot: ListViewWidget as WidgetComponent,
	pager: ChildrenPassthrough as WidgetComponent,
	pagination: ChildrenPassthrough as WidgetComponent,
	breadcrumb: ChildrenPassthrough as WidgetComponent,
	menu: ChildrenPassthrough as WidgetComponent,
	menuitem: ButtonWidget as WidgetComponent,
	html: FallbackWidget as WidgetComponent,
	popover: FallbackWidget as WidgetComponent,
	tooltip: FallbackWidget as WidgetComponent,
	progress: SkeletonWidget as WidgetComponent,
	badge: FieldWidget as WidgetComponent,
	avatar: FieldWidget as WidgetComponent
};

export function resolveWidget(type: string): WidgetComponent {
	return widgetRegistry[type] ?? (FallbackWidget as WidgetComponent);
}
