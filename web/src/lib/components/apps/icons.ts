import type { Component } from 'svelte';
import MessageSquareIcon from '@lucide/svelte/icons/message-square';
import UsersIcon from '@lucide/svelte/icons/users';
import UserIcon from '@lucide/svelte/icons/user';
import Building2Icon from '@lucide/svelte/icons/building-2';
import FileTextIcon from '@lucide/svelte/icons/file-text';
import LayoutDashboardIcon from '@lucide/svelte/icons/layout-dashboard';
import SettingsIcon from '@lucide/svelte/icons/settings';
import CalendarIcon from '@lucide/svelte/icons/calendar';
import MailIcon from '@lucide/svelte/icons/mail';
import ShoppingCartIcon from '@lucide/svelte/icons/shopping-cart';
import PackageIcon from '@lucide/svelte/icons/package';
import CreditCardIcon from '@lucide/svelte/icons/credit-card';
import ChartBarIcon from '@lucide/svelte/icons/chart-bar';
import ClipboardListIcon from '@lucide/svelte/icons/clipboard-list';
import BriefcaseIcon from '@lucide/svelte/icons/briefcase';
import BookOpenIcon from '@lucide/svelte/icons/book-open';
import TruckIcon from '@lucide/svelte/icons/truck';
import WrenchIcon from '@lucide/svelte/icons/wrench';
import FolderIcon from '@lucide/svelte/icons/folder';
import GlobeIcon from '@lucide/svelte/icons/globe';
import HeartPulseIcon from '@lucide/svelte/icons/heart-pulse';
import GraduationCapIcon from '@lucide/svelte/icons/graduation-cap';
import PaletteIcon from '@lucide/svelte/icons/palette';
import ShieldIcon from '@lucide/svelte/icons/shield';
import BoxesIcon from '@lucide/svelte/icons/boxes';
import ReceiptIcon from '@lucide/svelte/icons/receipt';
import BanknoteIcon from '@lucide/svelte/icons/banknote';
import PlugIcon from '@lucide/svelte/icons/plug';
import AppWindowIcon from '@lucide/svelte/icons/app-window';

/**
 * Icons an app can name (`icon = "…"` in its `[app]`). The set is fixed so the
 * app bundles only these; to offer another icon, add it here.
 */
export const appIcons: Record<string, Component<{ class?: string }>> = {
		'message-square': MessageSquareIcon,
		'users': UsersIcon,
		'user': UserIcon,
		'building-2': Building2Icon,
		'file-text': FileTextIcon,
		'layout-dashboard': LayoutDashboardIcon,
		'settings': SettingsIcon,
		'calendar': CalendarIcon,
		'mail': MailIcon,
		'shopping-cart': ShoppingCartIcon,
		'package': PackageIcon,
		'credit-card': CreditCardIcon,
		'chart-bar': ChartBarIcon,
		'clipboard-list': ClipboardListIcon,
		'briefcase': BriefcaseIcon,
		'book-open': BookOpenIcon,
		'truck': TruckIcon,
		'wrench': WrenchIcon,
		'folder': FolderIcon,
		'globe': GlobeIcon,
		'heart-pulse': HeartPulseIcon,
		'graduation-cap': GraduationCapIcon,
		'palette': PaletteIcon,
		'shield': ShieldIcon,
		'boxes': BoxesIcon,
		'receipt': ReceiptIcon,
		'banknote': BanknoteIcon,
		'plug': PlugIcon,
		'app-window': AppWindowIcon
};

/** The icon for `name`, or undefined when the app names one we do not ship. */
export function appIcon(name: string | null | undefined): Component<{ class?: string }> | undefined {
	return name ? appIcons[name] : undefined;
}
