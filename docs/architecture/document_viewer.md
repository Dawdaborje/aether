---
title: "Universal document viewer"
description: "One viewer in the web app for PDF, Office files, spreadsheets, images, text, audio and video, with a safe way for plugins to add their own viewers in JavaScript."
section: architecture
status: stable
order: 210
---
# Universal document viewer

**Status: built.** The registry and viewers (image with zoom and rotate; PDF drawn by pdf.js with page thumbnails, zoom and search; Word with headings, lists, emphasis and tables; Excel with every sheet; PowerPoint slide text; CSV; text and JSON; Markdown; sandboxed HTML; `.eml` messages with attachment list; audio and video; zip listing; download card), the reusable `ViewerPane`, the `<viewer>` page widget, the `/view/<key>` full-page route, the side preview panel that steps through files, plugin viewers (`[[viewer]]`, `GET /api/ui/viewers`, the sandboxed frame) and the `viewers.plugin_enabled` setting. Not built: highlighting the search hits on the page (the viewer jumps to the page), and the conversion bridge for exact Office layout. Choices are marked *Decide* in the last section.

Before this, `DocumentViewer.svelte` chose among a fixed set of kinds in `viewKind()` and a plugin could not add one:
plugin screens are XML, and the browser sees only built-in widgets ([Components](../plugin/components.md)). Now
`web/src/lib/media/viewers/` holds a registry (`registry.ts`), the built-in viewers (`builtin.ts`) and the plugin
loader (`plugin.ts`); `ViewerPane.svelte` picks the viewer and `DocumentViewer.svelte` is its dialog. How a plugin ships
one is in [Plugin viewers](../plugin/viewers.md).

Every file-heavy module (the Atrium drive, chatter attachments, employee documents) needs the same viewer, and some
plugins will own formats the kernel should not know about (drawings, 3D models, medical images, a company format).

## Built-in viewers

One registry, one contract. Each viewer is a component that receives the file's metadata, a URL, and a function that
fetches its bytes.

| Kind | Behaviour |
|---|---|
| Image | zoom, rotate, fit; SVG shown as an image, never inline |
| PDF | pages drawn by pdf.js (loaded only when a PDF is opened) with search, thumbnails, zoom and print |
| Word documents | rendered as sanitized HTML, not only extracted text |
| Spreadsheets (xlsx, ods, csv) | sheets as tabs, a virtualized grid, formulas shown as their cached value |
| Presentations | slide list with the text of each slide |
| Text, code, Markdown, JSON | highlighting; Markdown rendered through the sanitizer |. Markdown has Preview and Source modes and zoom (buttons, Ctrl with the wheel or + and -); it fills the width of its window by default
| Audio and video | the browser's player |
| Archives | listing of entries, no extraction |
| Email files (.eml) | headers, body and attachment list |
| HTML | in a sandboxed frame without scripts |
| Anything else | a card with name, size, type and a download button |

The "Extracted text" action of [OCR](ocr.md) stays on the toolbar for kinds that support it.

## Used everywhere

- The `FileField` widget, record attachments and chatter files open the same viewer.
- The `<viewer>` XML widget for pages that show a file inline ([Pages](../plugin/pages.md)), and a full-screen route `/view/<media key>`.
- A preview panel (a side sheet) used by lists, so a user can step through files without leaving the list. Chat messages use the same panel as a near full-screen modal (`mode="modal"`), with next and previous between the files of a message. In chat, a file in a message is drawn as a file card (a tile for its kind, name, kind and size, a download button) and opens in the modal on click.
- Access is the file's own: the viewer only fetches what the user may read through [Media](media.md).

## Plugin viewers in JavaScript

A plugin declares a viewer in `plugin.toml` and ships the script:

```toml
[[viewer]]
name = "dwg"
label = "%viewer_dwg%"
mimes = ["image/vnd.dwg"]
extensions = ["dwg", "dxf"]
module = "viewers/dwg.js"
priority = 50
```

The script is an ES module served from the plugin's assets. It never runs in the page of the web app:

1. The web app opens a frame with the `sandbox="allow-scripts"` attribute and no `allow-same-origin`: the script has
   an empty origin, no cookies, and cannot call the kernel API.
2. The two sides talk with `postMessage`: `init` (file name, type, size, and the bytes as a transferable or a
   short-lived blob URL), `resize`, `ready`, `error`, and `action` (ask the host to download, print or extract text).
3. The host decides what an action may do; the script cannot navigate the page or open windows.
4. An organization setting turns plugin viewers off. A content-security policy for the frame limits network access
   (none by default; a viewer may declare hosts it needs, and an administrator sees them when installing).

This keeps the rule of the [gaps roadmap](gaps_roadmap.md): a plugin adds declared data and sandboxed code, never
unchecked code in the app's own origin or in the server.

### Which viewer opens a file

The highest `priority` among the viewers that match. Built-in viewers use 100 and the download card -1000; a plugin
viewer defaults to 50, so it opens types nobody else claims, and takes over a built-in type only when it asks for a
higher number. Matching is by exact type, a family (`image/*`) or extension.

## Later

Co-editing of Office files is built another way: the `collabora` bridge makes an address that opens the file in an external editor
(Collabora Online or any WOPI editor) in a new tab, and the kernel answers the editor's WOPI calls; each save becomes a new version in
the drive (see [Bridges](../bridges.md)). Showing the editor inside the viewer's frame is not built.

## Decide

| # | Question | Proposal |
|---|---|---|
| 1 | Render Office files in the browser or convert on the server? | **In the browser** with sanitized output; a server conversion bridge can be added for fidelity later. |
| 2 | May a plugin viewer reach the network? | **No by default**; hosts are declared and shown to an administrator. |
| 3 | Where does PDF rendering come from? | pdf.js, a bundled dependency loaded only when a PDF is opened. |
| 4 | Can a plugin replace a built-in viewer? | **Yes**, by a higher `priority` for the same type. An organization switch to pin the built-in is not built. |
