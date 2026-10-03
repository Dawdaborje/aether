/** A button an error page can offer in place of its default "go to the start". */
export interface ErrorAction {
	label: string;
	href: string;
}

/** What every error page receives. */
export interface ErrorPageProps {
	/** HTTP status (or a status-like code such as 500 for an unexpected failure). */
	status: number;
	/** Overrides the standard title for this status. */
	title?: string;
	/** Overrides the standard message for this status. */
	message?: string;
	/** Extra technical detail (a request id, a path); shown small. */
	detail?: string;
	/** The main button; where to go from here. Defaults to "go to the start". */
	action?: ErrorAction;
}

/** What an inline error card receives. */
export interface ErrorCardProps {
	title: string;
	message?: string;
}
