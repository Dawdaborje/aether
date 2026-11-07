export interface NavLink {
	title: string;
	isActive?: boolean;
	items?: {
		title: string;
		url: string;
		icon: string;
		isActive?: boolean;
	}[];
}
