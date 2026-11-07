//! The OpenAPI description of a plugin's record API, generated from its model definitions.
//!
//! There is no second source: the schemas come from the model files (types, required fields, choices,
//! limits), the paths from the plugin's `[[api]]` blocks, and what the calling person may do from their
//! rules (a field they cannot read is not described; one they cannot write is read-only). The description
//! therefore cannot drift from the behaviour. See `docs/architecture/api_and_webhooks.md`.

use std::collections::HashSet;

use serde_json::{Map, Value, json};

use crate::data_model::{FieldDef, FieldType, ModelDef};

/// One model of the plugin that the API offers, as the caller sees it.
pub struct Exposed<'a> {
    pub model: &'a ModelDef,
    /// The operations to describe: the `[[api]]` ones that the caller's token and rules leave.
    pub operations: Vec<&'static str>,
    /// Fields the caller may not read (left out) ...
    pub hidden: HashSet<String>,
    /// ... and may not write (read-only).
    pub locked: HashSet<String>,
}

fn schema_of(field: &FieldDef) -> Option<Value> {
    let mut schema = match field.kind {
        FieldType::String | FieldType::Text => {
            let mut s = json!({ "type": "string" });
            if let Some(max) = field.max_length {
                s["maxLength"] = json!(max);
            }
            if let Some(min) = field.min_length {
                s["minLength"] = json!(min);
            }
            s
        }
        FieldType::Int => json!({ "type": "integer" }),
        FieldType::Float => json!({ "type": "number" }),
        FieldType::Decimal => json!({
            "type": "string",
            "pattern": "^-?[0-9]+(\\.[0-9]+)?$",
            "description": format!("An exact decimal with {} digits after the point, as text.", field.scale.unwrap_or(2)),
        }),
        FieldType::Bool => json!({ "type": "boolean" }),
        FieldType::Date => json!({ "type": "string", "format": "date" }),
        FieldType::Datetime => json!({ "type": "string", "format": "date-time" }),
        FieldType::Select => {
            let values: Vec<&str> = field.options.iter().map(|o| o.value.as_str()).collect();
            json!({ "type": "string", "enum": values })
        }
        FieldType::Link => json!({
            "type": "string",
            "description": format!(
                "The id (`table:key`) of a record of `{}`.",
                field.target.as_deref().unwrap_or("another model")
            ),
        }),
        FieldType::Json => json!({ "description": "Any JSON." }),
        FieldType::Image | FieldType::File => json!({
            "type": if field.multiple { "array" } else { "object" },
            "description": "A file held in the organization's media storage.",
        }),
        // Kept as edges or as rows of another model: not a value of this record.
        FieldType::Many2many | FieldType::Child => return None,
    };
    if let Some(help) = &field.help {
        if schema.get("description").is_none() {
            schema["description"] = json!(help);
        }
    }
    if field.deprecated {
        schema["deprecated"] = json!(true);
    }
    Some(schema)
}

/// The components for one model: `<Name>` as read and `<Name>Input` as written.
fn schemas(entry: &Exposed<'_>) -> (Value, Value) {
    let mut read = Map::new();
    read.insert("id".into(), json!({ "type": "string", "readOnly": true, "description": "The record's id (`table:key`)." }));
    let (mut write, mut required) = (Map::new(), Vec::new());
    for field in &entry.model.fields {
        if entry.hidden.contains(&field.name) {
            continue;
        }
        let Some(mut schema) = schema_of(field) else {
            continue;
        };
        if let Some(label) = &field.label {
            schema["title"] = json!(label);
        }
        if entry.locked.contains(&field.name) {
            let mut as_read = schema.clone();
            as_read["readOnly"] = json!(true);
            read.insert(field.name.clone(), as_read);
            continue;
        }
        read.insert(field.name.clone(), schema.clone());
        write.insert(field.name.clone(), schema);
        if field.required && field.default.is_none() {
            required.push(field.name.clone());
        }
    }
    let read = json!({ "type": "object", "properties": read });
    let mut input = json!({ "type": "object", "properties": write, "additionalProperties": false });
    if !required.is_empty() {
        input["required"] = json!(required);
    }
    (read, input)
}

fn camel(name: &str) -> String {
    name.split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or(String::new(), |c| c.to_uppercase().chain(chars).collect())
        })
        .collect()
}

fn error_response(description: &str) -> Value {
    json!({ "description": description, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Error" } } } })
}

/// The OpenAPI 3.1 document for the models of `plugin` that `exposed` lists.
pub fn document(plugin: &str, version: &str, exposed: &[Exposed<'_>]) -> Value {
    let mut components = Map::new();
    components.insert(
        "Error".into(),
        json!({
            "type": "object",
            "required": ["error"],
            "properties": { "error": { "type": "object", "required": ["code", "message"], "properties": {
                "code": { "type": "string", "enum": ["unauthenticated", "denied", "token_scope", "not_found", "invalid", "rate_limited", "internal"] },
                "message": { "type": "string" } } } },
        }),
    );
    let mut paths = Map::new();
    for entry in exposed.iter().filter(|e| !e.operations.is_empty()) {
        let name = &entry.model.name;
        let title = camel(name);
        let (read, input) = schemas(entry);
        components.insert(title.clone(), read);
        components.insert(format!("{title}Input"), input);
        let item = json!({ "$ref": format!("#/components/schemas/{title}") });
        let body = json!({ "required": true, "content": { "application/json": { "schema": { "$ref": format!("#/components/schemas/{title}Input") } } } });
        let id = json!({ "name": "id", "in": "path", "required": true, "schema": { "type": "string" }, "description": "The record's id, with or without the `table:` part." });
        let has = |op: &str| entry.operations.contains(&op);
        let mut collection = Map::new();
        if has("list") {
            collection.insert("get".into(), json!({
                "operationId": format!("list{title}"),
                "summary": format!("List {name} records"),
                "parameters": [
                    { "name": "filter", "in": "query", "schema": { "type": "string" }, "description": "A filter as JSON (see the Queries page): {\"status\": \"open\", \"points\": {\"gte\": 5}}." },
                    { "name": "fields", "in": "query", "schema": { "type": "string" }, "description": "Comma-separated field names to return." },
                    { "name": "limit", "in": "query", "schema": { "type": "integer", "minimum": 1, "maximum": 200, "default": 50 } },
                    { "name": "after", "in": "query", "schema": { "type": "string" }, "description": "The `next` of the previous page." }
                ],
                "responses": {
                    "200": { "description": "A page of records, ordered by id.", "content": { "application/json": { "schema": {
                        "type": "object", "properties": { "data": { "type": "array", "items": item }, "next": { "type": ["string", "null"] } } } } } },
                    "401": error_response("No valid credential."), "403": error_response("Not allowed."), "422": error_response("A bad filter.")
                }
            }));
        }
        if has("create") {
            collection.insert("post".into(), json!({
                "operationId": format!("create{title}"),
                "summary": format!("Create a {name} record"),
                "requestBody": body,
                "responses": {
                    "201": { "description": "Created.", "content": { "application/json": { "schema": { "type": "object", "properties": { "data": item } } } } },
                    "401": error_response("No valid credential."), "403": error_response("Not allowed."), "422": error_response("Invalid data, or a rule refused it.")
                }
            }));
        }
        if !collection.is_empty() {
            paths.insert(format!("/api/v1/{plugin}/{name}"), Value::Object(collection));
        }
        let mut single = Map::new();
        single.insert("parameters".into(), json!([id]));
        if has("get") {
            single.insert("get".into(), json!({
                "operationId": format!("get{title}"),
                "summary": format!("Read a {name} record"),
                "responses": {
                    "200": { "description": "The record.", "content": { "application/json": { "schema": { "type": "object", "properties": { "data": item } } } } },
                    "401": error_response("No valid credential."), "404": error_response("No such record, or you may not read it.")
                }
            }));
        }
        if has("update") {
            single.insert("patch".into(), json!({
                "operationId": format!("update{title}"),
                "summary": format!("Change a {name} record"),
                "requestBody": body,
                "responses": {
                    "200": { "description": "The record as it is now.", "content": { "application/json": { "schema": { "type": "object", "properties": { "data": item } } } } },
                    "403": error_response("Not allowed."), "404": error_response("No such record."), "422": error_response("Invalid data, or a rule refused it.")
                }
            }));
        }
        if has("delete") {
            single.insert("delete".into(), json!({
                "operationId": format!("delete{title}"),
                "summary": format!("Delete a {name} record"),
                "responses": {
                    "200": { "description": "Deleted.", "content": { "application/json": { "schema": { "type": "object", "properties": { "deleted": { "type": "string" } } } } } },
                    "403": error_response("Not allowed."), "404": error_response("No such record.")
                }
            }));
        }
        if single.len() > 1 {
            paths.insert(format!("/api/v1/{plugin}/{name}/{{id}}"), Value::Object(single));
        }
    }
    json!({
        "openapi": "3.1.0",
        "info": { "title": format!("{plugin} record API"), "version": version,
                  "description": "Generated from the plugin's models, rules and `[[api]]` blocks. Describes what the calling credential can do." },
        "servers": [{ "url": "/" }],
        "security": [{ "bearerAuth": [] }],
        "paths": paths,
        "components": {
            "securitySchemes": { "bearerAuth": { "type": "http", "scheme": "bearer", "description": "An API token (`aet_…`) made under your profile." } },
            "schemas": components,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn model() -> ModelDef {
        serde_json::from_value(json!({
            "name": "sales_lead",
            "fields": [
                { "name": "title", "type": "string", "required": true, "max_length": 80, "label": "Title" },
                { "name": "status", "type": "select", "default": "new", "options": [{ "value": "new" }, { "value": "won" }] },
                { "name": "amount", "type": "decimal", "scale": 2 },
                { "name": "salary", "type": "int" },
                { "name": "owner", "type": "link", "target": "party.party" },
                { "name": "skills", "type": "many2many", "target": "skill" },
                { "name": "closed", "type": "date" }
            ]
        }))
        .expect("a model")
    }

    fn entry<'a>(model: &'a ModelDef, ops: &[&'static str], hidden: &[&str], locked: &[&str]) -> Exposed<'a> {
        Exposed {
            model,
            operations: ops.to_vec(),
            hidden: hidden.iter().map(|s| s.to_string()).collect(),
            locked: locked.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn schemas_come_from_the_model() {
        let m = model();
        let doc = document("crm", "1.2.0", &[entry(&m, &["list", "get", "create", "update", "delete"], &[], &[])]);
        assert_eq!(doc["openapi"], "3.1.0");
        let read = &doc["components"]["schemas"]["SalesLead"]["properties"];
        assert_eq!(read["title"]["maxLength"], 80);
        assert_eq!(read["status"]["enum"], json!(["new", "won"]));
        assert_eq!(read["amount"]["type"], "string");
        assert_eq!(read["closed"]["format"], "date");
        assert!(read["owner"]["description"].as_str().is_some_and(|d| d.contains("party.party")));
        assert!(read.get("skills").is_none(), "relations are not values of the record");
        assert_eq!(read["id"]["readOnly"], true);
        // Only a field with no default is required on create.
        assert_eq!(doc["components"]["schemas"]["SalesLeadInput"]["required"], json!(["title"]));
        assert_eq!(doc["components"]["schemas"]["SalesLeadInput"]["additionalProperties"], false);
        assert!(doc["components"]["schemas"]["SalesLeadInput"]["properties"].get("id").is_none());
    }

    #[test]
    fn paths_follow_the_operations_the_caller_has() {
        let m = model();
        let all = document("crm", "1", &[entry(&m, &["list", "get", "create", "update", "delete"], &[], &[])]);
        let collection = &all["paths"]["/api/v1/crm/sales_lead"];
        assert!(collection.get("get").is_some() && collection.get("post").is_some());
        let single = &all["paths"]["/api/v1/crm/sales_lead/{id}"];
        assert!(single.get("get").is_some() && single.get("patch").is_some() && single.get("delete").is_some());

        let read_only = document("crm", "1", &[entry(&m, &["list", "get"], &[], &[])]);
        assert!(read_only["paths"]["/api/v1/crm/sales_lead"].get("post").is_none());
        let single = &read_only["paths"]["/api/v1/crm/sales_lead/{id}"];
        assert!(single.get("get").is_some() && single.get("patch").is_none() && single.get("delete").is_none());

        let nothing = document("crm", "1", &[entry(&m, &[], &[], &[])]);
        assert!(nothing["paths"].as_object().is_some_and(|p| p.is_empty()), "a model the caller cannot use is not described");
        assert!(nothing["components"]["schemas"].get("SalesLead").is_none());
    }

    #[test]
    fn what_the_caller_cannot_see_or_write_is_not_described_as_such() {
        let m = model();
        let doc = document("crm", "1", &[entry(&m, &["list", "get", "create"], &["salary"], &["status"])]);
        let read = &doc["components"]["schemas"]["SalesLead"]["properties"];
        let input = &doc["components"]["schemas"]["SalesLeadInput"]["properties"];
        assert!(read.get("salary").is_none() && input.get("salary").is_none(), "hidden: nowhere");
        assert_eq!(read["status"]["readOnly"], true);
        assert!(input.get("status").is_none(), "locked: not in the input");
        assert!(input.get("title").is_some());
    }

    #[test]
    fn the_document_names_its_security_and_errors() {
        let m = model();
        let doc = document("crm", "1", &[entry(&m, &["list"], &[], &[])]);
        assert_eq!(doc["components"]["securitySchemes"]["bearerAuth"]["scheme"], "bearer");
        assert_eq!(doc["security"], json!([{ "bearerAuth": [] }]));
        assert_eq!(doc["components"]["schemas"]["Error"]["required"], json!(["error"]));
        assert_eq!(doc["paths"]["/api/v1/crm/sales_lead"]["get"]["operationId"], "listSalesLead");
        assert!(doc["components"]["schemas"].get("securitySchemes").is_none());
    }
}
