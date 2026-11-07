-- What the models of the installed plugins look like in this organization's database: one
-- row per model, keyed by its id. `snapshot` lists every field by id (including hidden ones,
-- whose columns and data are kept) with the type, required flag and index applied. An
-- upgrade is planned by comparing a model's definition with its snapshot.
DEFINE TABLE IF NOT EXISTS model_schema;
DEFINE FIELD IF NOT EXISTS model_id ON TABLE model_schema TYPE string;
DEFINE FIELD IF NOT EXISTS plugin ON TABLE model_schema TYPE string;
DEFINE FIELD IF NOT EXISTS name ON TABLE model_schema TYPE string;
DEFINE FIELD IF NOT EXISTS snapshot ON TABLE model_schema TYPE object;
DEFINE FIELD IF NOT EXISTS applied_at ON TABLE model_schema TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS model_schema_model_id ON TABLE model_schema COLUMNS model_id UNIQUE;
