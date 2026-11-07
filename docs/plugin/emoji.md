---
title: "Emoji packs"
description: "A plugin can register emoji packs; any message box of the organization offers them."
section: plugin
status: stable
order: 46
---
# Emoji packs

Emoji are a registry, not something each plugin carries. A plugin that has emoji declares them in `plugin.toml`:

```toml
[[emoji_pack]]
name = "emojibase"        # lower-case letters, digits, `_` and `-`
label = "Emoji"
file = "emoji/pack.json"  # a .json file inside the plugin's assets/ folder
```

Any plugin that has a message box asks for them:

```xml
<composer send="post_message" emoji="true"/>
```

`emoji="true"` offers the packs of every enabled plugin, merged: groups with the same `key` share a tab and an emoji
that two packs both have is offered once. `emoji="emojibase"` offers only the pack of that name. If no enabled plugin
registers a pack, the message box has no emoji button. So HR, Booking or any other plugin gets emoji without shipping
any, as long as a plugin that has them (Chat, by default) is enabled.

## The file

```json
{
  "groups": [{ "key": "smileys-emotion", "name": "smileys & emotion" }],
  "emoji": [{ "c": "😀", "n": "grinning face", "t": ["happy", "grin"], "s": ["grinning"], "g": 0, "k": ["…", "…", "…", "…", "…"] }]
}
```

`c` is the character, `n` its name, `t` words it can be found by, `s` the `:shortcodes` that type it, `g` the index of
its group and `k` the five skin-tone versions, light to dark. `t`, `s` and `k` are optional.

## How it is served

`GET /api/ui/emoji` lists the packs of the enabled plugins of the organization (`{ packs: [{ plugin, name, label,
url }] }`); `url` is the file under `/api/ui/plugins/<plugin>/assets/`. The file is served as it is, so no plugin code
runs and nothing counts against a function's fuel; the web app fetches each pack once per page load.

The manifest is checked when the plugin loads: a bad name, a `file` outside `assets/` or not ending in `.json`, or two
packs with the same name refuses the plugin with the reason.

The Chat plugin's pack is built from [Emojibase](https://github.com/milesj/emojibase) by `tools/build_emoji.py`.
