import type { PDFDocumentProxy } from 'pdfjs-dist';

/** pdf.js is large, so it is fetched only when a PDF is opened, and only once. */
let library: Promise<typeof import('pdfjs-dist')> | null = null;

export function loadPdfLibrary() {
	library ??= (async () => {
		const [pdf, worker] = await Promise.all([
			import('pdfjs-dist'),
			import('pdfjs-dist/build/pdf.worker.min.mjs?url')
		]);
		pdf.GlobalWorkerOptions.workerSrc = worker.default;
		return pdf;
	})();
	library.catch(() => (library = null));
	return library;
}

export async function openPdf(bytes: ArrayBuffer): Promise<PDFDocumentProxy> {
	const pdf = await loadPdfLibrary();
	// pdf.js takes the buffer over, so it gets a copy and the file stays readable for other actions.
	return pdf.getDocument({ data: new Uint8Array(bytes.slice(0)) }).promise;
}

/** The pages (1-based) of `doc` that contain `query`, once per occurrence, in reading order. */
export async function searchPdf(
	doc: PDFDocumentProxy,
	query: string,
	isCancelled: () => boolean,
	limit = 500
): Promise<number[]> {
	const needle = query.trim().toLowerCase();
	const hits: number[] = [];
	if (!needle) return hits;
	for (let n = 1; n <= doc.numPages && hits.length < limit; n++) {
		if (isCancelled()) return hits;
		const page = await doc.getPage(n);
		const content = await page.getTextContent();
		const text = content.items
			.map((item) => ('str' in item ? item.str : ''))
			.join(' ')
			.toLowerCase();
		let at = text.indexOf(needle);
		while (at !== -1 && hits.length < limit) {
			hits.push(n);
			at = text.indexOf(needle, at + needle.length);
		}
	}
	return hits;
}
