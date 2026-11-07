# Aether: working rules

## Where documentation lives

- `docs/` (this repo) is **kernel documentation only**: the kernel, the platform features every
  plugin can use (events, rules, scheduler, Mantle, themes, the SFU, ...), the plugin-authoring
  guide (`docs/plugin/`) and the user guides. It must make sense without any particular plugin
  installed. Plugins may appear in it only as short examples.
- Anything that describes one plugin or one plugin suite (its models, roles, rules, roadmap,
  design, platform gaps, integration with other plugins) belongs in that plugin's own repo under
  `../plugins/`:
  - a single plugin: `<workspace>/<plugin>/docs/` (for example `plugins/base/party/docs/`);
  - a suite or workspace: `<workspace>/docs/` (for example `plugins/atrium/docs/`,
    `plugins/lms/docs/`).
- Do not write a plugin's design into `docs/architecture/` "for now". If a feature is a general
  kernel capability that a plugin merely needs, document the capability in `docs/architecture/`
  and the plugin's use of it in the plugin.
- Plugin docs cannot link into `docs/` with relative paths (they live in other repos): name the
  kernel page in plain text instead.
