//! Excel and PDF tables from rows a report already produced.

use serde_json::{Map, Value};

use crate::spec::ReportError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Xlsx,
    Pdf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportColumn {
    pub field: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportTable {
    pub format: ExportFormat,
    pub title: String,
    pub columns: Vec<ExportColumn>,
    pub rows: Vec<Map<String, Value>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportFile {
    pub filename: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

pub fn export_table(table: &ExportTable) -> Result<ExportFile, ReportError> {
    if table.columns.is_empty() {
        return Err(ReportError::Invalid("export needs at least one column".into()));
    }
    if table.columns.len() > 32 {
        return Err(ReportError::Invalid("at most 32 columns".into()));
    }
    if table.rows.len() > 1000 {
        return Err(ReportError::Invalid("at most 1000 rows".into()));
    }
    let stem = slug(&table.title);
    match table.format {
        ExportFormat::Xlsx => Ok(ExportFile {
            filename: format!("{stem}.xlsx"),
            content_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".into(),
            bytes: xlsx(table)?,
        }),
        ExportFormat::Pdf => Ok(ExportFile {
            filename: format!("{stem}.pdf"),
            content_type: "application/pdf".into(),
            bytes: pdf(table),
        }),
    }
}

pub fn parse_export(value: &Value) -> Result<ExportTable, ReportError> {
    let object = value
        .as_object()
        .ok_or_else(|| ReportError::Invalid("export must be an object".into()))?;
    let format = match object.get("format").and_then(Value::as_str).unwrap_or("") {
        "xlsx" | "excel" => ExportFormat::Xlsx,
        "pdf" => ExportFormat::Pdf,
        other => {
            return Err(ReportError::Invalid(format!(
                "format must be xlsx or pdf, not `{other}`"
            )))
        }
    };
    let title = object
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Report")
        .to_string();
    let columns = object
        .get("columns")
        .and_then(Value::as_array)
        .ok_or_else(|| ReportError::Invalid("columns are required".into()))?
        .iter()
        .map(|column| {
            let object = column
                .as_object()
                .ok_or_else(|| ReportError::Invalid("a column is { field, label }".into()))?;
            let field = object
                .get("field")
                .and_then(Value::as_str)
                .ok_or_else(|| ReportError::Invalid("column.field is required".into()))?;
            let label = object
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or(field);
            Ok(ExportColumn { field: field.to_string(), label: label.to_string() })
        })
        .collect::<Result<Vec<_>, ReportError>>()?;
    let rows = object
        .get("rows")
        .and_then(Value::as_array)
        .ok_or_else(|| ReportError::Invalid("rows are required".into()))?
        .iter()
        .filter_map(Value::as_object)
        .cloned()
        .collect();
    Ok(ExportTable { format, title, columns, rows })
}

fn slug(title: &str) -> String {
    let mut out = String::new();
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-');
    if out.is_empty() { "report" } else { out }.to_string()
}

fn cell(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(flag) => flag.to_string(),
        other => other.to_string(),
    }
}

fn xlsx(table: &ExportTable) -> Result<Vec<u8>, ReportError> {
    let mut book = rust_xlsxwriter::Workbook::new();
    let sheet = book.add_worksheet();
    sheet
        .write_string(0, 0, &table.title)
        .map_err(|e| ReportError::Invalid(e.to_string()))?;
    for (col, column) in table.columns.iter().enumerate() {
        sheet
            .write_string(2, col as u16, &column.label)
            .map_err(|e| ReportError::Invalid(e.to_string()))?;
    }
    for (row_index, row) in table.rows.iter().enumerate() {
        for (col, column) in table.columns.iter().enumerate() {
            let text = row.get(&column.field).map(cell).unwrap_or_default();
            sheet
                .write_string(3 + row_index as u32, col as u16, &text)
                .map_err(|e| ReportError::Invalid(e.to_string()))?;
        }
    }
    book.save_to_buffer()
        .map_err(|e| ReportError::Invalid(e.to_string()))
}

/// A single-page PDF table using the built-in Helvetica font.
fn pdf(table: &ExportTable) -> Vec<u8> {
    let mut lines = vec![table.title.clone(), String::new()];
    lines.push(
        table
            .columns
            .iter()
            .map(|c| c.label.clone())
            .collect::<Vec<_>>()
            .join("  |  "),
    );
    for row in &table.rows {
        lines.push(
            table
                .columns
                .iter()
                .map(|c| row.get(&c.field).map(cell).unwrap_or_default())
                .collect::<Vec<_>>()
                .join("  |  "),
        );
    }
    // Keep the page short: one line is about 14pt, letter height is 792.
    lines.truncate(48);
    let mut content = String::new();
    content.push_str("BT /F1 11 Tf 48 740 Td 14 TL\n");
    for line in &lines {
        content.push('(');
        content.push_str(&pdf_text(line));
        content.push_str(") Tj T*\n");
    }
    content.push_str("ET\n");

    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{}endstream", content.len(), content),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.push_str(&format!("{} 0 obj\n{}\nendobj\n", index + 1, object));
    }
    let xref = out.len();
    out.push_str(&format!("xref\n0 {}\n", objects.len() + 1));
    out.push_str("0000000000 65535 f \n");
    for offset in offsets {
        out.push_str(&format!("{offset:010} 00000 n \n"));
    }
    out.push_str(&format!(
        "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF",
        objects.len() + 1
    ));
    out.into_bytes()
}

fn pdf_text(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars().take(120) {
        match ch {
            '\\' | '(' | ')' => {
                out.push('\\');
                out.push(ch);
            }
            c if c.is_ascii() && !c.is_control() => out.push(c),
            _ => out.push('?'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample(format: &str) -> ExportTable {
        parse_export(&json!({
            "format": format,
            "title": "Funnel",
            "columns": [{ "field": "name", "label": "Stage" }, { "field": "n", "label": "Deals" }],
            "rows": [{ "name": "New", "n": 3 }]
        }))
        .unwrap()
    }

    #[test]
    fn xlsx_and_pdf_are_files() {
        let xlsx = export_table(&sample("xlsx")).unwrap();
        assert!(xlsx.filename.ends_with(".xlsx"));
        assert!(xlsx.bytes.starts_with(b"PK"));
        let pdf = export_table(&sample("pdf")).unwrap();
        assert!(pdf.bytes.starts_with(b"%PDF"));
    }
}
