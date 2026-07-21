use axum::{
    Json, Router,
    extract::Path,
    http::StatusCode,
    routing::get,
};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Serialize)]
pub struct PageResponse {
    pub route: String,
    pub title: String,
    pub source: String,
    pub page: Value,
}

pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new().route("/pages/{*slug}", get(get_page))
}

async fn get_page(Path(slug): Path<String>) -> Result<Json<PageResponse>, StatusCode> {
    let route = format!("/{slug}");
    Ok(Json(PageResponse {
        route: route.clone(),
        title: "Invoices".into(),
        source: "demo".into(),
        page: demo_invoices_page(&route),
    }))
}

fn demo_invoices_page(route: &str) -> Value {
    json!({
        "type": "page",
        "route": route,
        "title": "Invoices",
        "children": [
            {
                "type": "header",
                "title": "Invoices",
                "children": [
                    {
                        "type": "actions",
                        "children": [
                            { "type": "button", "name": "create", "label": "Create", "variant": "default" },
                            { "type": "button", "name": "export", "label": "Export", "variant": "outline" }
                        ]
                    }
                ]
            },
            {
                "type": "notebook",
                "children": [
                    {
                        "type": "page",
                        "title": "List",
                        "children": [
                            {
                                "type": "view",
                                "viewType": "list",
                                "model": "accounting.invoice",
                                "children": [
                                    {
                                        "type": "search",
                                        "placeholder": "Search invoices…"
                                    },
                                    {
                                        "type": "columns",
                                        "children": [
                                            { "type": "column", "field": "number", "label": "Invoice #", "sortable": true },
                                            { "type": "column", "field": "partner", "label": "Customer", "fieldType": "many2one" },
                                            { "type": "column", "field": "amount_total", "label": "Total", "fieldType": "currency" },
                                            { "type": "column", "field": "status", "label": "Status", "fieldType": "badge" }
                                        ]
                                    },
                                    {
                                        "type": "actions",
                                        "children": [
                                            { "type": "action", "name": "post", "label": "Post", "confirm": true },
                                            { "type": "action", "name": "cancel", "label": "Cancel", "danger": true, "confirm": true }
                                        ]
                                    },
                                    {
                                        "type": "empty",
                                        "title": "No invoices yet",
                                        "description": "Create your first invoice to get started.",
                                        "actionLabel": "Create invoice"
                                    }
                                ]
                            }
                        ]
                    },
                    {
                        "type": "page",
                        "title": "Form",
                        "children": [
                            {
                                "type": "view",
                                "viewType": "form",
                                "model": "accounting.invoice",
                                "children": [
                                    {
                                        "type": "group",
                                        "title": "Invoice details",
                                        "children": [
                                            { "type": "field", "name": "number", "label": "Invoice #", "fieldType": "char", "required": true },
                                            { "type": "field", "name": "partner", "label": "Customer", "fieldType": "many2one" },
                                            { "type": "field", "name": "amount_total", "label": "Total", "fieldType": "currency" },
                                            { "type": "field", "name": "status", "label": "Status", "fieldType": "selection", "options": ["draft", "posted", "cancelled"] },
                                            { "type": "field", "name": "notes", "label": "Notes", "fieldType": "text" }
                                        ]
                                    },
                                    {
                                        "type": "buttonbox",
                                        "children": [
                                            { "type": "button", "name": "save", "label": "Save" },
                                            { "type": "button", "name": "discard", "label": "Discard", "variant": "outline" }
                                        ]
                                    }
                                ]
                            }
                        ]
                    },
                    {
                        "type": "page",
                        "title": "Kanban",
                        "children": [
                            {
                                "type": "view",
                                "viewType": "kanban",
                                "model": "accounting.invoice",
                                "children": [
                                    {
                                        "type": "group",
                                        "title": "Draft",
                                        "children": [
                                            { "type": "stat", "label": "INV/2026/001", "value": "$1,200.00" }
                                        ]
                                    },
                                    {
                                        "type": "group",
                                        "title": "Posted",
                                        "children": [
                                            { "type": "stat", "label": "INV/2026/002", "value": "$890.00" }
                                        ]
                                    }
                                ]
                            }
                        ]
                    }
                ]
            },
            {
                "type": "modal",
                "name": "create_invoice",
                "title": "Create invoice",
                "children": [
                    { "type": "field", "name": "partner", "label": "Customer", "fieldType": "char" },
                    { "type": "field", "name": "amount", "label": "Amount", "fieldType": "float" },
                    {
                        "type": "actions",
                        "children": [
                            { "type": "button", "name": "confirm_create", "label": "Create" },
                            { "type": "button", "name": "close", "label": "Cancel", "variant": "outline" }
                        ]
                    }
                ]
            },
            {
                "type": "confirm",
                "name": "post_confirm",
                "title": "Post invoice?",
                "description": "Posted invoices cannot be edited.",
                "confirmLabel": "Post",
                "cancelLabel": "Keep draft"
            },
            { "type": "spinner", "label": "Loading invoices…" },
            { "type": "toast", "name": "saved", "title": "Saved", "description": "Invoice updated." }
        ]
    })
}
