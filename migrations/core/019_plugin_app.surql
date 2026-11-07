-- A plugin that is an *app* (a tile on the Apps launcher) declares `[app]` in its
-- manifest: { label, icon?, route, description? }. `route` is a page route of the
-- plugin that the tile opens.
DEFINE FIELD IF NOT EXISTS app ON TABLE plugins TYPE option<object | NONE>;
