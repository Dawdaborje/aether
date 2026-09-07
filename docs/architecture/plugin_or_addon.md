# Aether plugin design

So a plugin is a wasm compile with it's frontend components for the program extensibility of the aether

## Configuration
All the configuration of the aether plugins/addons in the plugin.toml

## Models
Models will be generated and stored as json, which this will help aether manage and be able to alter columns.
Field types will be the same as the supported in the surrealdb.

## UI
UI will be written in xml (small dsl) and then compile it to json tree for the frontend to consume.
