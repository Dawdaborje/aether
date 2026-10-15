import type { Component } from 'svelte';
import AppWindowIcon from '@lucide/svelte/icons/app-window';
import ArrowUpRightIcon from '@lucide/svelte/icons/arrow-up-right';
import BanknoteIcon from '@lucide/svelte/icons/banknote';
import BellIcon from '@lucide/svelte/icons/bell';
import BookOpenIcon from '@lucide/svelte/icons/book-open';
import BoxesIcon from '@lucide/svelte/icons/boxes';
import BriefcaseIcon from '@lucide/svelte/icons/briefcase';
import Building2Icon from '@lucide/svelte/icons/building-2';
import BuildingIcon from '@lucide/svelte/icons/building';
import CalendarCheckIcon from '@lucide/svelte/icons/calendar-check';
import CalendarDaysIcon from '@lucide/svelte/icons/calendar-days';
import CalendarIcon from '@lucide/svelte/icons/calendar';
import ChartBarIcon from '@lucide/svelte/icons/chart-bar';
import ChartColumnIcon from '@lucide/svelte/icons/chart-column';
import CheckSquareIcon from '@lucide/svelte/icons/check-square';
import CircleXIcon from '@lucide/svelte/icons/circle-x';
import ClipboardListIcon from '@lucide/svelte/icons/clipboard-list';
import ClockIcon from '@lucide/svelte/icons/clock';
import CoinsIcon from '@lucide/svelte/icons/coins';
import ContactIcon from '@lucide/svelte/icons/contact';
import CreditCardIcon from '@lucide/svelte/icons/credit-card';
import DoorOpenIcon from '@lucide/svelte/icons/door-open';
import FactoryIcon from '@lucide/svelte/icons/factory';
import FileTextIcon from '@lucide/svelte/icons/file-text';
import FlameIcon from '@lucide/svelte/icons/flame';
import FolderIcon from '@lucide/svelte/icons/folder';
import FolderOpenIcon from '@lucide/svelte/icons/folder-open';
import FormInputIcon from '@lucide/svelte/icons/form-input';
import GiftIcon from '@lucide/svelte/icons/gift';
import GitBranchIcon from '@lucide/svelte/icons/git-branch';
import GlobeIcon from '@lucide/svelte/icons/globe';
import GraduationCapIcon from '@lucide/svelte/icons/graduation-cap';
import HandCoinsIcon from '@lucide/svelte/icons/hand-coins';
import HandshakeIcon from '@lucide/svelte/icons/handshake';
import HashIcon from '@lucide/svelte/icons/hash';
import HeartPulseIcon from '@lucide/svelte/icons/heart-pulse';
import LaptopIcon from '@lucide/svelte/icons/laptop';
import LayoutDashboardIcon from '@lucide/svelte/icons/layout-dashboard';
import MailIcon from '@lucide/svelte/icons/mail';
import MapIcon from '@lucide/svelte/icons/map';
import MapPinIcon from '@lucide/svelte/icons/map-pin';
import MegaphoneIcon from '@lucide/svelte/icons/megaphone';
import MessageSquareIcon from '@lucide/svelte/icons/message-square';
import MessagesSquareIcon from '@lucide/svelte/icons/messages-square';
import PackageIcon from '@lucide/svelte/icons/package';
import PaletteIcon from '@lucide/svelte/icons/palette';
import PhoneIcon from '@lucide/svelte/icons/phone';
import PlaneIcon from '@lucide/svelte/icons/plane';
import PlugIcon from '@lucide/svelte/icons/plug';
import ReceiptIcon from '@lucide/svelte/icons/receipt';
import ScaleIcon from '@lucide/svelte/icons/scale';
import SettingsIcon from '@lucide/svelte/icons/settings';
import ShieldIcon from '@lucide/svelte/icons/shield';
import ShoppingBagIcon from '@lucide/svelte/icons/shopping-bag';
import ShoppingCartIcon from '@lucide/svelte/icons/shopping-cart';
import TagIcon from '@lucide/svelte/icons/tag';
import TargetIcon from '@lucide/svelte/icons/target';
import TruckIcon from '@lucide/svelte/icons/truck';
import UserIcon from '@lucide/svelte/icons/user';
import UserPlusIcon from '@lucide/svelte/icons/user-plus';
import UsersIcon from '@lucide/svelte/icons/users';
import UsersRoundIcon from '@lucide/svelte/icons/users-round';
import WarehouseIcon from '@lucide/svelte/icons/warehouse';
import WrenchIcon from '@lucide/svelte/icons/wrench';

/**
 * Icons an app or page can name (`[app] icon` and `<page icon>`). The set is
 * fixed so the app bundles only these; the sidebar draws a link's icon from
 * this map. To offer another icon, add it here.
 */
export const appIcons: Record<string, Component<{ class?: string }>> = {
	'app-window': AppWindowIcon,
	'arrow-up-right': ArrowUpRightIcon,
	banknote: BanknoteIcon,
	bell: BellIcon,
	'book-open': BookOpenIcon,
	boxes: BoxesIcon,
	briefcase: BriefcaseIcon,
	building: BuildingIcon,
	'building-2': Building2Icon,
	calendar: CalendarIcon,
	'calendar-check': CalendarCheckIcon,
	'calendar-days': CalendarDaysIcon,
	'chart-bar': ChartBarIcon,
	'chart-column': ChartColumnIcon,
	'check-square': CheckSquareIcon,
	'circle-x': CircleXIcon,
	'clipboard-list': ClipboardListIcon,
	clock: ClockIcon,
	cog: SettingsIcon,
	coins: CoinsIcon,
	contact: ContactIcon,
	'credit-card': CreditCardIcon,
	'door-open': DoorOpenIcon,
	factory: FactoryIcon,
	'file-text': FileTextIcon,
	flame: FlameIcon,
	folder: FolderIcon,
	'folder-open': FolderOpenIcon,
	'git-branch': GitBranchIcon,
	'form-input': FormInputIcon,
	gift: GiftIcon,
	globe: GlobeIcon,
	'graduation-cap': GraduationCapIcon,
	'hand-coins': HandCoinsIcon,
	handshake: HandshakeIcon,
	hash: HashIcon,
	'heart-pulse': HeartPulseIcon,
	laptop: LaptopIcon,
	'layout-dashboard': LayoutDashboardIcon,
	map: MapIcon,
	mail: MailIcon,
	'map-pin': MapPinIcon,
	megaphone: MegaphoneIcon,
	'message-square': MessageSquareIcon,
	'messages-square': MessagesSquareIcon,
	package: PackageIcon,
	palette: PaletteIcon,
	phone: PhoneIcon,
	plane: PlaneIcon,
	plug: PlugIcon,
	receipt: ReceiptIcon,
	scale: ScaleIcon,
	settings: SettingsIcon,
	shield: ShieldIcon,
	'shopping-bag': ShoppingBagIcon,
	'shopping-cart': ShoppingCartIcon,
	tag: TagIcon,
	target: TargetIcon,
	truck: TruckIcon,
	user: UserIcon,
	'user-plus': UserPlusIcon,
	users: UsersIcon,
	'users-round': UsersRoundIcon,
	warehouse: WarehouseIcon,
	wrench: WrenchIcon
};

/** The icon for `name`, or undefined when the app names one we do not ship. */
export function appIcon(name: string | null | undefined): Component<{ class?: string }> | undefined {
	return name ? appIcons[name] : undefined;
}
