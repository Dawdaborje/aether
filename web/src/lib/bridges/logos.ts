import { base } from '$app/paths';

/** Public URL for a bridge logo under `static/bridges/<feature_key>.svg`. */
export function bridgeLogoUrl(featureKey: string): string {
	return `${base}/bridges/${encodeURIComponent(featureKey)}.svg`;
}
