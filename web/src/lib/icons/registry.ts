import type { Component } from 'svelte';
import ActivityIcon from 'carbon-icons-svelte/lib/Activity.svelte';
import AddIcon from 'carbon-icons-svelte/lib/Add.svelte';
import AlarmIcon from 'carbon-icons-svelte/lib/Alarm.svelte';
import ApiIcon from 'carbon-icons-svelte/lib/Api.svelte';
import ApplicationIcon from 'carbon-icons-svelte/lib/Application.svelte';
import AppsIcon from 'carbon-icons-svelte/lib/Apps.svelte';
import ArchiveIcon from 'carbon-icons-svelte/lib/Archive.svelte';
import ArrowDownIcon from 'carbon-icons-svelte/lib/ArrowDown.svelte';
import ArrowDownRightIcon from 'carbon-icons-svelte/lib/ArrowDownRight.svelte';
import ArrowLeftIcon from 'carbon-icons-svelte/lib/ArrowLeft.svelte';
import ArrowRightIcon from 'carbon-icons-svelte/lib/ArrowRight.svelte';
import ArrowUpRightIcon from 'carbon-icons-svelte/lib/ArrowUpRight.svelte';
import AtIcon from 'carbon-icons-svelte/lib/At.svelte';
import AttachmentIcon from 'carbon-icons-svelte/lib/Attachment.svelte';
import BlogIcon from 'carbon-icons-svelte/lib/Blog.svelte';
import BookIcon from 'carbon-icons-svelte/lib/Book.svelte';
import BoxIcon from 'carbon-icons-svelte/lib/Box.svelte';
import BranchIcon from 'carbon-icons-svelte/lib/Branch.svelte';
import BuildingIcon from 'carbon-icons-svelte/lib/Building.svelte';
import BullhornIcon from 'carbon-icons-svelte/lib/Bullhorn.svelte';
import CalendarIcon from 'carbon-icons-svelte/lib/Calendar.svelte';
import CalendarHeatMapIcon from 'carbon-icons-svelte/lib/CalendarHeatMap.svelte';
import CarIcon from 'carbon-icons-svelte/lib/Car.svelte';
import CatalogIcon from 'carbon-icons-svelte/lib/Catalog.svelte';
import CharacterWholeNumberIcon from 'carbon-icons-svelte/lib/CharacterWholeNumber.svelte';
import ChartBarIcon from 'carbon-icons-svelte/lib/ChartBar.svelte';
import ChartColumnIcon from 'carbon-icons-svelte/lib/ChartColumn.svelte';
import ChatIcon from 'carbon-icons-svelte/lib/Chat.svelte';
import CheckboxCheckedIcon from 'carbon-icons-svelte/lib/CheckboxChecked.svelte';
import CheckmarkIcon from 'carbon-icons-svelte/lib/Checkmark.svelte';
import CheckmarkFilledIcon from 'carbon-icons-svelte/lib/CheckmarkFilled.svelte';
import CheckmarkOutlineIcon from 'carbon-icons-svelte/lib/CheckmarkOutline.svelte';
import ChevronDownIcon from 'carbon-icons-svelte/lib/ChevronDown.svelte';
import ChevronLeftIcon from 'carbon-icons-svelte/lib/ChevronLeft.svelte';
import ChevronRightIcon from 'carbon-icons-svelte/lib/ChevronRight.svelte';
import ChevronSortIcon from 'carbon-icons-svelte/lib/ChevronSort.svelte';
import ChevronUpIcon from 'carbon-icons-svelte/lib/ChevronUp.svelte';
import CloseIcon from 'carbon-icons-svelte/lib/Close.svelte';
import CloseOutlineIcon from 'carbon-icons-svelte/lib/CloseOutline.svelte';
import CloudOfflineIcon from 'carbon-icons-svelte/lib/CloudOffline.svelte';
import ColorPaletteIcon from 'carbon-icons-svelte/lib/ColorPalette.svelte';
import ColumnIcon from 'carbon-icons-svelte/lib/Column.svelte';
import CurrencyIcon from 'carbon-icons-svelte/lib/Currency.svelte';
import Cursor1Icon from 'carbon-icons-svelte/lib/Cursor_1.svelte';
import CustomerServiceIcon from 'carbon-icons-svelte/lib/CustomerService.svelte';
import DashboardIcon from 'carbon-icons-svelte/lib/Dashboard.svelte';
import DataBaseIcon from 'carbon-icons-svelte/lib/DataBase.svelte';
import Data2Icon from 'carbon-icons-svelte/lib/Data_2.svelte';
import DeliveryParcelIcon from 'carbon-icons-svelte/lib/DeliveryParcel.svelte';
import DeliveryTruckIcon from 'carbon-icons-svelte/lib/DeliveryTruck.svelte';
import DocumentIcon from 'carbon-icons-svelte/lib/Document.svelte';
import DocumentExportIcon from 'carbon-icons-svelte/lib/DocumentExport.svelte';
import DocumentTasksIcon from 'carbon-icons-svelte/lib/DocumentTasks.svelte';
import DocumentViewIcon from 'carbon-icons-svelte/lib/DocumentView.svelte';
import DownloadIcon from 'carbon-icons-svelte/lib/Download.svelte';
import EarthFilledIcon from 'carbon-icons-svelte/lib/EarthFilled.svelte';
import EditIcon from 'carbon-icons-svelte/lib/Edit.svelte';
import EducationIcon from 'carbon-icons-svelte/lib/Education.svelte';
import EmailIcon from 'carbon-icons-svelte/lib/Email.svelte';
import EnterpriseIcon from 'carbon-icons-svelte/lib/Enterprise.svelte';
import ErrorFilledIcon from 'carbon-icons-svelte/lib/ErrorFilled.svelte';
import EventScheduleIcon from 'carbon-icons-svelte/lib/EventSchedule.svelte';
import ExitIcon from 'carbon-icons-svelte/lib/Exit.svelte';
import ExpandScreenIcon from 'carbon-icons-svelte/lib/ExpandScreen.svelte';
import FaceAddIcon from 'carbon-icons-svelte/lib/FaceAdd.svelte';
import FaceSatisfiedIcon from 'carbon-icons-svelte/lib/FaceSatisfied.svelte';
import FilterIcon from 'carbon-icons-svelte/lib/Filter.svelte';
import FireIcon from 'carbon-icons-svelte/lib/Fire.svelte';
import FlagIcon from 'carbon-icons-svelte/lib/Flag.svelte';
import FolderIcon from 'carbon-icons-svelte/lib/Folder.svelte';
import FolderAddIcon from 'carbon-icons-svelte/lib/FolderAdd.svelte';
import FolderDetailsIcon from 'carbon-icons-svelte/lib/FolderDetails.svelte';
import FolderOpenIcon from 'carbon-icons-svelte/lib/FolderOpen.svelte';
import ForumIcon from 'carbon-icons-svelte/lib/Forum.svelte';
import GiftIcon from 'carbon-icons-svelte/lib/Gift.svelte';
import GroupIcon from 'carbon-icons-svelte/lib/Group.svelte';
import HeadsetIcon from 'carbon-icons-svelte/lib/Headset.svelte';
import HistoryIcon from 'carbon-icons-svelte/lib/History.svelte';
import HomeIcon from 'carbon-icons-svelte/lib/Home.svelte';
import ImageIcon from 'carbon-icons-svelte/lib/Image.svelte';
import InProgressIcon from 'carbon-icons-svelte/lib/InProgress.svelte';
import IndustryIcon from 'carbon-icons-svelte/lib/Industry.svelte';
import IntegrationIcon from 'carbon-icons-svelte/lib/Integration.svelte';
import InventoryManagementIcon from 'carbon-icons-svelte/lib/InventoryManagement.svelte';
import LaptopIcon from 'carbon-icons-svelte/lib/Laptop.svelte';
import LayersIcon from 'carbon-icons-svelte/lib/Layers.svelte';
import LightningIcon from 'carbon-icons-svelte/lib/Lightning.svelte';
import ListIcon from 'carbon-icons-svelte/lib/List.svelte';
import ListCheckedIcon from 'carbon-icons-svelte/lib/ListChecked.svelte';
import LocationIcon from 'carbon-icons-svelte/lib/Location.svelte';
import LockedIcon from 'carbon-icons-svelte/lib/Locked.svelte';
import MagicWandIcon from 'carbon-icons-svelte/lib/MagicWand.svelte';
import MapIcon from 'carbon-icons-svelte/lib/Map.svelte';
import MaximizeIcon from 'carbon-icons-svelte/lib/Maximize.svelte';
import MicrophoneIcon from 'carbon-icons-svelte/lib/Microphone.svelte';
import MicrophoneOffIcon from 'carbon-icons-svelte/lib/MicrophoneOff.svelte';
import MisuseIcon from 'carbon-icons-svelte/lib/Misuse.svelte';
import MoneyIcon from 'carbon-icons-svelte/lib/Money.svelte';
import MoonIcon from 'carbon-icons-svelte/lib/Moon.svelte';
import MusicIcon from 'carbon-icons-svelte/lib/Music.svelte';
import Network3Icon from 'carbon-icons-svelte/lib/Network_3.svelte';
import NotificationIcon from 'carbon-icons-svelte/lib/Notification.svelte';
import NotificationOffIcon from 'carbon-icons-svelte/lib/NotificationOff.svelte';
import OverflowMenuHorizontalIcon from 'carbon-icons-svelte/lib/OverflowMenuHorizontal.svelte';
import PackageIcon from 'carbon-icons-svelte/lib/Package.svelte';
import PartnershipIcon from 'carbon-icons-svelte/lib/Partnership.svelte';
import PasswordIcon from 'carbon-icons-svelte/lib/Password.svelte';
import PercentageIcon from 'carbon-icons-svelte/lib/Percentage.svelte';
import PhoneIcon from 'carbon-icons-svelte/lib/Phone.svelte';
import PhoneOffIcon from 'carbon-icons-svelte/lib/PhoneOff.svelte';
import PinIcon from 'carbon-icons-svelte/lib/Pin.svelte';
import PlaneIcon from 'carbon-icons-svelte/lib/Plane.svelte';
import PlugIcon from 'carbon-icons-svelte/lib/Plug.svelte';
import PortfolioIcon from 'carbon-icons-svelte/lib/Portfolio.svelte';
import PresentationFileIcon from 'carbon-icons-svelte/lib/PresentationFile.svelte';
import PrinterIcon from 'carbon-icons-svelte/lib/Printer.svelte';
import PurchaseIcon from 'carbon-icons-svelte/lib/Purchase.svelte';
import RadioIcon from 'carbon-icons-svelte/lib/Radio.svelte';
import ReceiptIcon from 'carbon-icons-svelte/lib/Receipt.svelte';
import RenewIcon from 'carbon-icons-svelte/lib/Renew.svelte';
import RepeatIcon from 'carbon-icons-svelte/lib/Repeat.svelte';
import ReplyIcon from 'carbon-icons-svelte/lib/Reply.svelte';
import RestartIcon from 'carbon-icons-svelte/lib/Restart.svelte';
import RotateClockwiseIcon from 'carbon-icons-svelte/lib/RotateClockwise.svelte';
import RotateCounterclockwiseIcon from 'carbon-icons-svelte/lib/RotateCounterclockwise.svelte';
import SaveIcon from 'carbon-icons-svelte/lib/Save.svelte';
import ScalesIcon from 'carbon-icons-svelte/lib/Scales.svelte';
import ScanIcon from 'carbon-icons-svelte/lib/Scan.svelte';
import ScreenIcon from 'carbon-icons-svelte/lib/Screen.svelte';
import SearchIcon from 'carbon-icons-svelte/lib/Search.svelte';
import SearchLocateIcon from 'carbon-icons-svelte/lib/SearchLocate.svelte';
import SecurityIcon from 'carbon-icons-svelte/lib/Security.svelte';
import SecurityServicesIcon from 'carbon-icons-svelte/lib/SecurityServices.svelte';
import SendIcon from 'carbon-icons-svelte/lib/Send.svelte';
import SettingsIcon from 'carbon-icons-svelte/lib/Settings.svelte';
import ShoppingBagIcon from 'carbon-icons-svelte/lib/ShoppingBag.svelte';
import ShoppingCartIcon from 'carbon-icons-svelte/lib/ShoppingCart.svelte';
import SidePanelOpenIcon from 'carbon-icons-svelte/lib/SidePanelOpen.svelte';
import StarIcon from 'carbon-icons-svelte/lib/Star.svelte';
import SubtractIcon from 'carbon-icons-svelte/lib/Subtract.svelte';
import SunIcon from 'carbon-icons-svelte/lib/Sun.svelte';
import TableIcon from 'carbon-icons-svelte/lib/Table.svelte';
import TagIcon from 'carbon-icons-svelte/lib/Tag.svelte';
import TargetIcon from 'carbon-icons-svelte/lib/Target.svelte';
import TaskIcon from 'carbon-icons-svelte/lib/Task.svelte';
import TaskCompleteIcon from 'carbon-icons-svelte/lib/TaskComplete.svelte';
import TemplateIcon from 'carbon-icons-svelte/lib/Template.svelte';
import TextLongParagraphIcon from 'carbon-icons-svelte/lib/TextLongParagraph.svelte';
import TicketIcon from 'carbon-icons-svelte/lib/Ticket.svelte';
import TimeIcon from 'carbon-icons-svelte/lib/Time.svelte';
import TimerIcon from 'carbon-icons-svelte/lib/Timer.svelte';
import ToolsIcon from 'carbon-icons-svelte/lib/Tools.svelte';
import TrashCanIcon from 'carbon-icons-svelte/lib/TrashCan.svelte';
import TreeViewIcon from 'carbon-icons-svelte/lib/TreeView.svelte';
import UndoIcon from 'carbon-icons-svelte/lib/Undo.svelte';
import UploadIcon from 'carbon-icons-svelte/lib/Upload.svelte';
import UserIcon from 'carbon-icons-svelte/lib/User.svelte';
import UserCertificationIcon from 'carbon-icons-svelte/lib/UserCertification.svelte';
import UserFollowIcon from 'carbon-icons-svelte/lib/UserFollow.svelte';
import UserMinusIcon from 'carbon-icons-svelte/lib/UserMinus.svelte';
import UserMultipleIcon from 'carbon-icons-svelte/lib/UserMultiple.svelte';
import UserProfileIcon from 'carbon-icons-svelte/lib/UserProfile.svelte';
import VideoIcon from 'carbon-icons-svelte/lib/Video.svelte';
import VideoOffIcon from 'carbon-icons-svelte/lib/VideoOff.svelte';
import ViewIcon from 'carbon-icons-svelte/lib/View.svelte';
import ViewOffIcon from 'carbon-icons-svelte/lib/ViewOff.svelte';
import WalletIcon from 'carbon-icons-svelte/lib/Wallet.svelte';
import WarningAltIcon from 'carbon-icons-svelte/lib/WarningAlt.svelte';
import WarningFilledIcon from 'carbon-icons-svelte/lib/WarningFilled.svelte';
import WebhookIcon from 'carbon-icons-svelte/lib/Webhook.svelte';
import ZoomInIcon from 'carbon-icons-svelte/lib/ZoomIn.svelte';
import ZoomOutIcon from 'carbon-icons-svelte/lib/ZoomOut.svelte';

/**
 * The default icon set: IBM Carbon, by name.
 *
 * This is the only module that imports Carbon. Components draw icons by name with
 * `<AppIcon name="…" />`, which uses the active theme's symbol for that name when its sprite
 * has one (see "Icons" in docs/architecture/themes.md) and this set otherwise. Apps, pages
 * and nav items name icons from the same list. To add an icon, add it here; the name list in
 * the docs is checked against this map by `icons.spec.ts`.
 */
export const defaultIcons = {
	'app-window': ApplicationIcon,
	'arrow-up-right': ArrowUpRightIcon,
	banknote: MoneyIcon,
	bell: NotificationIcon,
	'book-open': BookIcon,
	boxes: BoxIcon,
	briefcase: PortfolioIcon,
	building: BuildingIcon,
	'building-2': EnterpriseIcon,
	calendar: CalendarIcon,
	'calendar-check': EventScheduleIcon,
	'calendar-days': CalendarIcon,
	'chart-bar': ChartBarIcon,
	'chart-column': ChartColumnIcon,
	'check-square': CheckboxCheckedIcon,
	'circle-x': CloseOutlineIcon,
	'clipboard-list': TaskIcon,
	clock: TimeIcon,
	cog: SettingsIcon,
	coins: MoneyIcon,
	contact: UserProfileIcon,
	'credit-card': PurchaseIcon,
	'door-open': ExitIcon,
	factory: IndustryIcon,
	'file-text': DocumentIcon,
	flame: FireIcon,
	folder: FolderIcon,
	'folder-open': FolderOpenIcon,
	'git-branch': BranchIcon,
	'form-input': TextLongParagraphIcon,
	gift: GiftIcon,
	globe: EarthFilledIcon,
	'graduation-cap': EducationIcon,
	'hand-coins': WalletIcon,
	handshake: PartnershipIcon,
	'hard-drive': Data2Icon,
	hash: CharacterWholeNumberIcon,
	'heart-pulse': ActivityIcon,
	kanban: ColumnIcon,
	laptop: LaptopIcon,
	'layout-dashboard': DashboardIcon,
	'layout-grid': AppsIcon,
	map: MapIcon,
	mail: EmailIcon,
	'map-pin': LocationIcon,
	megaphone: BullhornIcon,
	'message-square': ChatIcon,
	'messages-square': ForumIcon,
	newspaper: BlogIcon,
	package: PackageIcon,
	palette: ColorPaletteIcon,
	phone: PhoneIcon,
	plane: PlaneIcon,
	plug: PlugIcon,
	receipt: ReceiptIcon,
	scale: ScalesIcon,
	settings: SettingsIcon,
	shield: SecurityIcon,
	'shopping-bag': ShoppingBagIcon,
	'shopping-cart': ShoppingCartIcon,
	tag: TagIcon,
	target: TargetIcon,
	truck: DeliveryTruckIcon,
	user: UserIcon,
	'user-plus': UserFollowIcon,
	users: UserMultipleIcon,
	'users-round': GroupIcon,
	video: VideoIcon,
	warehouse: InventoryManagementIcon,
	wrench: ToolsIcon,
	'badge-dollar-sign': CurrencyIcon,
	'calendar-range': CalendarHeatMapIcon,
	car: CarIcon,
	'check-check': CheckmarkOutlineIcon,
	'clipboard-check': TaskCompleteIcon,
	'file-check': DocumentTasksIcon,
	'folder-kanban': FolderDetailsIcon,
	'folder-tree': TreeViewIcon,
	house: HomeIcon,
	inbox: EmailIcon,
	key: PasswordIcon,
	list: ListIcon,
	network: Network3Icon,
	percent: PercentageIcon,
	puzzle: IntegrationIcon,
	'refresh-cw': RenewIcon,
	repeat: RepeatIcon,
	'rotate-ccw': RotateCounterclockwiseIcon,
	'shield-check': SecurityServicesIcon,
	'sliders-horizontal': SettingsIcon,
	timer: TimerIcon,
	'trending-down': ArrowDownRightIcon,
	'user-check': UserCertificationIcon,
	'user-x': UserMinusIcon,
	wallet: WalletIcon,
	home: HomeIcon,
	sliders: SettingsIcon,
	'x-circle': CloseOutlineIcon,
	book: BookIcon,
	check: CheckmarkIcon,
	'file-up': DocumentExportIcon,
	library: CatalogIcon,
	'list-checks': ListCheckedIcon,
	'list-tree': TreeViewIcon,
	pencil: EditIcon,
	'pencil-line': EditIcon,
	plus: AddIcon,
	presentation: PresentationFileIcon,
	star: StarIcon,
	'alarm-clock': AlarmIcon,
	'at-sign': AtIcon,
	ban: MisuseIcon,
	'file-clock': DocumentTasksIcon,
	files: DocumentIcon,
	flag: FlagIcon,
	headset: HeadsetIcon,
	history: HistoryIcon,
	'life-buoy': CustomerServiceIcon,
	'message-circle-heart': FaceSatisfiedIcon,
	'message-square-quote': ChatIcon,
	'mouse-pointer-click': Cursor1Icon,
	'pen-line': EditIcon,
	search: SearchIcon,
	smile: FaceSatisfiedIcon,
	ticket: TicketIcon,
	zap: LightningIcon,
	sparkles: MagicWandIcon,
	'key-round': ApiIcon,
	'scan-text': ScanIcon,
	'file-search': DocumentViewIcon,
	'file-spreadsheet': DocumentExportIcon,
	radio: RadioIcon,
	layout: TemplateIcon,
	webhook: WebhookIcon,
	archive: ArchiveIcon,
	'arrow-down': ArrowDownIcon,
	'arrow-left': ArrowLeftIcon,
	'arrow-right': ArrowRightIcon,
	attachment: AttachmentIcon,
	'check-circle-filled': CheckmarkFilledIcon,
	'chevron-down': ChevronDownIcon,
	'chevron-left': ChevronLeftIcon,
	'chevron-right': ChevronRightIcon,
	'chevron-up': ChevronUpIcon,
	'chevrons-up-down': ChevronSortIcon,
	close: CloseIcon,
	'cloud-off': CloudOfflineIcon,
	database: DataBaseIcon,
	download: DownloadIcon,
	'error-filled': ErrorFilledIcon,
	'expand-screen': ExpandScreenIcon,
	'face-add': FaceAddIcon,
	filter: FilterIcon,
	'folder-add': FolderAddIcon,
	image: ImageIcon,
	'in-progress': InProgressIcon,
	layers: LayersIcon,
	lock: LockedIcon,
	maximize: MaximizeIcon,
	microphone: MicrophoneIcon,
	'microphone-off': MicrophoneOffIcon,
	minus: SubtractIcon,
	monitor: ScreenIcon,
	moon: MoonIcon,
	'more-horizontal': OverflowMenuHorizontalIcon,
	music: MusicIcon,
	'notification-off': NotificationOffIcon,
	'package-open': DeliveryParcelIcon,
	'phone-off': PhoneOffIcon,
	pin: PinIcon,
	printer: PrinterIcon,
	refresh: RenewIcon,
	reply: ReplyIcon,
	restart: RestartIcon,
	'rotate-clockwise': RotateClockwiseIcon,
	save: SaveIcon,
	'search-x': SearchLocateIcon,
	send: SendIcon,
	'side-panel-open': SidePanelOpenIcon,
	sun: SunIcon,
	table: TableIcon,
	trash: TrashCanIcon,
	undo: UndoIcon,
	upload: UploadIcon,
	'video-off': VideoOffIcon,
	view: ViewIcon,
	'view-off': ViewOffIcon,
	warning: WarningAltIcon,
	'warning-filled': WarningFilledIcon,
	'zoom-in': ZoomInIcon,
	'zoom-out': ZoomOutIcon
} satisfies Record<string, Component<{ class?: string }>>;

/** A name in the default set. Themes may add names of their own. */
export type IconName = keyof typeof defaultIcons;
