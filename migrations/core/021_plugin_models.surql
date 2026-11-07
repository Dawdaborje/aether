-- Models are now defined in `models/<name>.json` files with stable ids (see data_model). The
-- catalog keeps each version's definitions here. A model's records live in the table named
-- after its `model_id`, which is the same for every version of the plugin, so the old
-- one-row-per-table uniqueness no longer applies.
DEFINE FIELD IF NOT EXISTS model_id ON TABLE plugin_models TYPE option<string | NONE>;
-- The model file as written: fields with their ids, options, view.
DEFINE FIELD IF NOT EXISTS definition ON TABLE plugin_models TYPE option<object | NONE>;
REMOVE INDEX IF EXISTS plugin_models_table_name ON TABLE plugin_models;
DEFINE INDEX IF NOT EXISTS plugin_models_model_id ON TABLE plugin_models COLUMNS model_id;

-- Where the package was loaded from, so a model edited in the web app can be written back to
-- its file (only on a machine that still has the package).
DEFINE FIELD IF NOT EXISTS source_path ON TABLE plugins TYPE option<string | NONE>;
