import type { Component } from 'svelte';
import type { MediaFile } from '../api';

/**
 * What a viewer gets to work with. A viewer never fetches by itself: it asks the context, so every
 * viewer (built in or supplied by a plugin) reads a file through the same checked path.
 */
export interface ViewerContext {
	file: MediaFile;
	/** A `blob:` URL the browser can show (images, PDF, audio, video). */
	objectUrl(): Promise<string>;
	/** The raw bytes, fetched once and kept while this file is open. */
	bytes(): Promise<ArrayBuffer>;
	/** The text of the file, cut so a huge file cannot freeze the page. */
	text(limit?: number): Promise<{ text: string; cut: boolean }>;
	/** Lets a viewer say how to print itself; without it the print button is hidden. */
	setPrint(print: (() => void) | null): void;
}

/** A viewer a plugin supplies (`[[viewer]]` in its `plugin.toml`): JavaScript that runs in a sandboxed frame. */
export interface PluginViewerSpec {
	plugin: string;
	name: string;
	/** Where the module is fetched from (the plugin's assets). */
	moduleUrl: string;
	/** Origins the module may fetch from; none unless the plugin asks and the manifest lists them. */
	hosts: string[];
}

export interface ViewerDef {
	/** Unique id; registering the same id again replaces the viewer. */
	id: string;
	label: string;
	/** Higher wins when several viewers match the same file. */
	priority: number;
	matches(file: Pick<MediaFile, 'name' | 'mime'>): boolean;
	component: Component<{ ctx: ViewerContext; spec?: PluginViewerSpec }>;
	/** Set for plugin viewers; handed to the component. */
	spec?: PluginViewerSpec;
	/** Whether the "Extracted text" action makes sense for this kind of file. */
	canExtract?: boolean;
}
