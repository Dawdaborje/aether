---
title: "GIF packs"
description: "A plugin can register GIF packs; any message box of the organization offers them."
section: plugin
status: stable
order: 47
---
# GIF packs

GIFs are a registry of their own, next to [emoji](emoji.md). A plugin that has GIFs declares them in `plugin.toml`:

```toml
[[gif_pack]]
name = "noto-animated"         # lower-case letters, digits, `_` and `-`
label = "Animated emoji"
file = "gif/pack.json"         # a .json file inside the plugin's assets/ folder
```

Any plugin that has a message box asks for them:

```xml
<composer send="post_message" emoji="true" gif="true" attach="true"/>
```

`gif="true"` offers the packs of every enabled plugin, merged; `gif="noto-animated"` offers only the pack of that name.
If no enabled plugin registers a pack, the message box has no GIF button. A GIF button opens a picker with a search box;
the GIF chosen is attached to the message like a file (it needs `attach` only for the paperclip, not for GIFs), with the
same preview card, and goes out with the next message. The people in the conversation see it as an image and can open it
in the viewer.

## The file

```json
{ "gifs": [
  { "n": "grinning face", "f": "gif/1f600.gif", "t": ["happy", "smile"], "s": ["grinning"] }
] }
```

`n` is the name, `f` the GIF's path inside the plugin's `assets/` folder, `t` words it can be found by and `s` the
`:shortcodes` it is known by; `t` and `s` are optional. A path that is absolute, has `..`, or is not plainly inside
`assets/` is left out.

## How it is served

`GET /api/ui/gifs` lists the packs of the enabled plugins of the organization (`{ packs: [{ plugin, name, label, url }] }`),
the same answer as `GET /api/ui/emoji`; the index and the GIFs are static files under `/api/ui/plugins/<plugin>/assets/`,
so no plugin code runs. The web app fetches the index once per page load and the GIFs as the picker shows them.

The manifest is checked when the plugin loads, like an emoji pack: a bad name, a `file` outside `assets/` or not ending in
`.json`, or two GIF packs with the same name refuses the plugin with the reason. An emoji pack and a GIF pack may share a
name.

The Chat plugin's pack is the animated Noto emoji (Google, CC BY 4.0), built by `tools/build_gifs.py`; it keeps the
attribution in `assets/gif/NOTICE.txt`.
