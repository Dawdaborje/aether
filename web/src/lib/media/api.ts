import { apiFetch } from '$lib/api/client';

/** What a record keeps for an uploaded file (see `data_model/media.rs` on the server). */
export interface MediaFile {
	key: string;
	name: string;
	mime: string;
	size: number;
	sha256?: string;
}

/** Largest file the server accepts. */
export const MAX_MEDIA_BYTES = 25 * 1024 * 1024;

export function isMediaFile(value: unknown): value is MediaFile {
	return (
		typeof value === 'object' &&
		value !== null &&
		typeof (value as MediaFile).key === 'string' &&
		(value as MediaFile).key.startsWith('media/')
	);
}

/** The files a field holds, whether it keeps one or a list. Old text values are not files. */
export function filesOf(value: unknown): MediaFile[] {
	if (Array.isArray(value)) return value.filter(isMediaFile);
	return isMediaFile(value) ? [value] : [];
}

/**
 * An image that is not an uploaded file: a data URL or web address written before the field
 * was an image field. Shown, and replaced when the person picks a new picture.
 */
export function legacyImageSrc(value: unknown): string {
	if (typeof value !== 'string') return '';
	return value.startsWith('data:image/') || /^https?:\/\//.test(value) ? value : '';
}

export function formatSize(bytes: number): string {
	if (bytes < 1024) return `${bytes} B`;
	if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
	return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

async function errorOf(response: Response): Promise<string> {
	try {
		const body = (await response.json()) as { error?: string };
		if (body.error) return body.error;
	} catch {
		/* not JSON */
	}
	return `The upload failed (${response.status}).`;
}

/** Upload one file; the result is what a record stores in an image or file field. */
export async function uploadMedia(file: File): Promise<MediaFile> {
	if (file.size === 0) throw new Error('That file is empty.');
	if (file.size > MAX_MEDIA_BYTES) {
		throw new Error(
			`That file is ${formatSize(file.size)}; the limit is ${formatSize(MAX_MEDIA_BYTES)}.`
		);
	}
	const response = await apiFetch(`/api/media?name=${encodeURIComponent(file.name)}`, {
		method: 'POST',
		headers: { 'content-type': file.type || 'application/octet-stream' },
		body: file
	});
	if (!response.ok) throw new Error(await errorOf(response));
	return (await response.json()) as MediaFile;
}

const urls = new Map<string, Promise<string>>();

/**
 * A URL a picture can be shown from. The file is fetched with the app's own request
 * helper (so the organization header and session go with it, which an `<img src>` cannot
 * do) and kept for the page's life: a key never changes what it holds.
 */
export function mediaObjectUrl(key: string, thumb = false): Promise<string> {
	const id = thumb ? `${key}?thumb=1` : key;
	let url = urls.get(id);
	if (!url) {
		url = apiFetch(`/api/media/${id}`).then(async (response) => {
			if (!response.ok) throw new Error(await errorOf(response));
			return URL.createObjectURL(await response.blob());
		});
		// A failure is not remembered, so the next look tries again.
		url.catch(() => urls.delete(id));
		urls.set(id, url);
	}
	return url;
}

/** Save a file to the person's disk. */
export async function downloadMedia(file: MediaFile): Promise<void> {
	const response = await apiFetch(`/api/media/${file.key}?name=${encodeURIComponent(file.name)}`);
	if (!response.ok) throw new Error(await errorOf(response));
	const url = URL.createObjectURL(await response.blob());
	const link = document.createElement('a');
	link.href = url;
	link.download = file.name;
	document.body.appendChild(link);
	link.click();
	link.remove();
	setTimeout(() => URL.revokeObjectURL(url), 10_000);
}
