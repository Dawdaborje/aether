---
title: "Document viewers"
description: "A plugin can add a viewer for a file type the web app does not know. It ships a JavaScript module that runs in a sandboxed frame."
section: plugin
status: stable
order: 45
---
# Document viewers

The web app opens files with a [universal document viewer](../architecture/document_viewer.md). It knows images,
PDF, Word, Excel, PowerPoint, CSV, text, audio, video and zip files. A plugin adds more with `[[viewer]]` in
`plugin.toml`:

```toml
[[viewer]]
name = "dwg"
label = "Drawing"
mimes = ["image/vnd.dwg"]     # exact types, or a family such as "model/*"
extensions = ["dwg", "dxf"]   # lower-case, without the dot
module = "viewers/dwg.js"     # inside the plugin's assets/ folder
priority = 50                 # 0 to 1000; built-in viewers use 100
hosts = []                    # https origins the module may fetch from; default none
```

Give `mimes`, `extensions` or both. The highest `priority` that matches a file wins, so the default `50` opens only
types without a built-in viewer. Use more than `100` to replace a built-in one.

## The module

An ES module in `assets/` (here `assets/viewers/dwg.js`). Its default export is called with a page element and a
description of the file:

```js
export default async function (root, { file, bytes, action }) {
  // file: { name, mime, size }    bytes: ArrayBuffer with the whole file
  root.textContent = `${file.name}: ${bytes.byteLength} bytes`;
  // action('download') and action('print') ask the app to do it for you.
}
```

## The sandbox

The module does not run in the app's page. It runs in a frame with `sandbox="allow-scripts"` and no
`allow-same-origin`, so it has an empty origin: no session, no cookies, no access to the app or to the Aether
API. A content-security policy blocks every request except to the `hosts` the manifest lists, and the frame can
open no other frames or windows. The app fetches the module and the file and passes them in with `postMessage`.

A module that throws, or a manifest that is wrong, shows the person an error in the viewer; the rest of the app is
not affected. The manifest is checked when the plugin loads: a bad name, a `module` outside `assets/` or not ending in
`.js` or `.mjs`, an unknown type form, a priority out of range or a `hosts` entry that is not an `https://` origin
refuses the plugin with the reason.

The list of viewers for an organization is `GET /api/ui/viewers`; it names only enabled plugins.

An administrator can switch plugin viewers off for the organization with the setting `viewers.plugin_enabled`
(Settings, Document viewers). Then `GET /api/ui/viewers` answers an empty list and only the built-in viewers open files.
