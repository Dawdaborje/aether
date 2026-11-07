//! Reading a spreadsheet file into rows of text: CSV (any of `,` `;` tab `|`, quoted fields, a byte order
//! mark, UTF-8 or the old Windows code page) and XLSX (the first sheet, or a named one).
//!
//! Everything is text here; what a cell means is decided by the field it is mapped to (see
//! [`super::mapping`]). A file is read within limits (rows, columns, and the size it unpacks to), so a
//! hostile file cannot use up the server.

use std::io::Read;

use quick_xml::{Reader, events::Event};
use thiserror::Error;

/// One row of the file, with its number in the file (the first row, the headers, is row 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub number: usize,
    pub cells: Vec<String>,
}

/// A file as headers and rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Row>,
    pub format: Format,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Csv { delimiter: char },
    Xlsx,
}

impl Format {
    pub fn name(self) -> &'static str {
        match self {
            Self::Csv { .. } => "csv",
            Self::Xlsx => "xlsx",
        }
    }
}

/// How much of a file is read.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_rows: usize,
    pub max_columns: usize,
    /// The most an XLSX may unpack to, in bytes.
    pub max_unpacked: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_rows: 1_000_000,
            max_columns: 200,
            max_unpacked: 256 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TableError {
    #[error("the file is empty")]
    Empty,
    #[error("the file has more than {0} rows")]
    TooManyRows(usize),
    #[error("the file has more than {0} columns")]
    TooManyColumns(usize),
    #[error("{0}")]
    Bad(String),
}

/// Read `bytes` as a table. The kind is decided by what the file is (an XLSX is a zip), not by its name;
/// `name` only helps to tell a text file that is not a CSV from one that is.
pub fn parse(bytes: &[u8], sheet: Option<&str>, limits: Limits) -> Result<Table, TableError> {
    if bytes.is_empty() {
        return Err(TableError::Empty);
    }
    if bytes.starts_with(b"PK\x03\x04") {
        let rows = xlsx_rows(bytes, sheet, &limits)?;
        return finish(rows, Format::Xlsx, &limits);
    }
    let text = decode(bytes);
    let delimiter = detect_delimiter(&text);
    let rows = csv_rows(&text, delimiter, &limits)?;
    finish(rows, Format::Csv { delimiter }, &limits)
}

/// The first row that has anything in it is the headers; rows that are entirely empty are dropped.
fn finish(mut rows: Vec<Row>, format: Format, limits: &Limits) -> Result<Table, TableError> {
    rows.retain(|row| row.cells.iter().any(|cell| !cell.trim().is_empty()));
    if rows.is_empty() {
        return Err(TableError::Empty);
    }
    let header_row = rows.remove(0);
    let width = header_row.cells.len();
    if width > limits.max_columns {
        return Err(TableError::TooManyColumns(limits.max_columns));
    }
    if rows.len() > limits.max_rows {
        return Err(TableError::TooManyRows(limits.max_rows));
    }
    let headers: Vec<String> = header_row
        .cells
        .iter()
        .enumerate()
        .map(|(index, header)| {
            let header = header.trim();
            if header.is_empty() {
                format!("column {}", index + 1)
            } else {
                header.to_string()
            }
        })
        .collect();
    // Every row is as wide as the headers: short rows are padded, extra cells are dropped.
    for row in &mut rows {
        row.cells.resize(width, String::new());
    }
    Ok(Table { headers, rows, format })
}

/// Text from bytes: UTF-8, without a byte order mark; or, if that is not valid, the Windows code page
/// 1252 that older spreadsheet programs write.
fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => bytes.iter().map(|byte| windows_1252(*byte)).collect(),
    }
}

fn windows_1252(byte: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘', '’', '“', '”',
        '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
    ];
    match byte {
        0x80..=0x9F => HIGH[(byte - 0x80) as usize],
        other => char::from(other),
    }
}

/// The separator the first lines use most, outside quotes.
fn detect_delimiter(text: &str) -> char {
    let mut best = (',', 0usize);
    for candidate in [',', ';', '\t', '|'] {
        let mut count = 0;
        let mut quoted = false;
        for c in text.chars().take(4096) {
            match c {
                '"' => quoted = !quoted,
                '\n' if !quoted => break,
                c if c == candidate && !quoted => count += 1,
                _ => {}
            }
        }
        if count > best.1 {
            best = (candidate, count);
        }
    }
    best.0
}

/// RFC 4180 and the usual leniency: quoted fields with `""` for a quote and newlines inside, `\n`, `\r\n`
/// or `\r` between rows.
fn csv_rows(text: &str, delimiter: char, limits: &Limits) -> Result<Vec<Row>, TableError> {
    let mut rows = Vec::new();
    let mut cells: Vec<String> = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut was_quoted = false;
    let mut chars = text.chars().peekable();
    let mut number = 1;
    let push_row = |rows: &mut Vec<Row>, cells: &mut Vec<String>, cell: &mut String, number: &mut usize| -> Result<(), TableError> {
        cells.push(std::mem::take(cell));
        if cells.len() > limits.max_columns {
            return Err(TableError::TooManyColumns(limits.max_columns));
        }
        rows.push(Row {
            number: *number,
            cells: std::mem::take(cells),
        });
        *number += 1;
        if rows.len() > limits.max_rows + 1 {
            return Err(TableError::TooManyRows(limits.max_rows));
        }
        Ok(())
    };
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cell.push('"');
                } else {
                    quoted = false;
                }
            } else {
                cell.push(c);
            }
            continue;
        }
        match c {
            '"' if cell.is_empty() && !was_quoted => {
                quoted = true;
                was_quoted = true;
            }
            c if c == delimiter => {
                cells.push(std::mem::take(&mut cell));
                was_quoted = false;
            }
            '\r' | '\n' => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                push_row(&mut rows, &mut cells, &mut cell, &mut number)?;
                was_quoted = false;
            }
            other => cell.push(other),
        }
    }
    if quoted {
        return Err(TableError::Bad("a quoted value is never closed".into()));
    }
    if !cell.is_empty() || !cells.is_empty() {
        push_row(&mut rows, &mut cells, &mut cell, &mut number)?;
    }
    Ok(rows)
}

fn bad(message: impl Into<String>) -> TableError {
    TableError::Bad(message.into())
}

/// An entry of the zip, read in full but never more than the limit allows.
fn entry(
    archive: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>,
    name: &str,
    budget: &mut u64,
) -> Result<Option<Vec<u8>>, TableError> {
    let Ok(file) = archive.by_name(name) else {
        return Ok(None);
    };
    if file.size() > *budget {
        return Err(bad("the spreadsheet unpacks to more than the limit allows"));
    }
    let mut out = Vec::new();
    file.take(*budget + 1)
        .read_to_end(&mut out)
        .map_err(|error| bad(format!("the spreadsheet could not be read: {error}")))?;
    if out.len() as u64 > *budget {
        return Err(bad("the spreadsheet unpacks to more than the limit allows"));
    }
    *budget -= out.len() as u64;
    Ok(Some(out))
}

fn attr(element: &quick_xml::events::BytesStart<'_>, name: &str) -> Option<String> {
    element
        .attributes()
        .flatten()
        .find(|a| a.key.as_ref() == name.as_bytes() || a.key.local_name().as_ref() == name.as_bytes())
        .and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()))
}

/// Text inside `<t>` elements, grouped by the element that holds them (`<si>` in the shared strings,
/// `<is>` in a cell). An entity or a character reference is resolved.
fn texts(xml: &[u8], group: &[u8]) -> Result<Vec<String>, TableError> {
    let mut reader = Reader::from_reader(xml);
    let mut out = Vec::new();
    let (mut current, mut in_group, mut in_t) = (String::new(), false, false);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.local_name();
                if name.as_ref() == group {
                    in_group = true;
                    current.clear();
                } else if name.as_ref() == b"t" && in_group {
                    in_t = true;
                }
            }
            Ok(Event::End(e)) => {
                let name = e.local_name();
                if name.as_ref() == b"t" {
                    in_t = false;
                } else if name.as_ref() == group && in_group {
                    in_group = false;
                    out.push(std::mem::take(&mut current));
                }
            }
            Ok(Event::Text(t)) if in_t => current.push_str(&t.decode().map_err(|e| bad(e.to_string()))?),
            Ok(Event::CData(t)) if in_t => current.push_str(&t.decode().map_err(|e| bad(e.to_string()))?),
            Ok(Event::GeneralRef(r)) if in_t => current.push(entity(&r)?),
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(bad(format!("the spreadsheet is not valid: {error}"))),
        }
    }
    Ok(out)
}

fn entity(reference: &quick_xml::events::BytesRef<'_>) -> Result<char, TableError> {
    if let Some(c) = reference.resolve_char_ref().map_err(|e| bad(e.to_string()))? {
        return Ok(c);
    }
    match reference.decode().map_err(|e| bad(e.to_string()))?.as_ref() {
        "lt" => Ok('<'),
        "gt" => Ok('>'),
        "amp" => Ok('&'),
        "apos" => Ok('\''),
        "quot" => Ok('"'),
        other => Err(bad(format!("unknown entity `&{other};` in the spreadsheet"))),
    }
}

/// A column letter like `AB` as an index from 0.
fn column_index(reference: &str) -> usize {
    reference
        .chars()
        .take_while(char::is_ascii_alphabetic)
        .fold(0usize, |n, c| n * 26 + (c.to_ascii_uppercase() as usize - 'A' as usize + 1))
        .saturating_sub(1)
}

fn xlsx_rows(bytes: &[u8], sheet: Option<&str>, limits: &Limits) -> Result<Vec<Row>, TableError> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| bad("this is not a spreadsheet file"))?;
    let mut budget = limits.max_unpacked;
    let workbook = entry(&mut archive, "xl/workbook.xml", &mut budget)?.ok_or_else(|| bad("this is not an XLSX spreadsheet"))?;
    let rels = entry(&mut archive, "xl/_rels/workbook.xml.rels", &mut budget)?.unwrap_or_default();

    // Which file is the sheet: the one named, or the first.
    let mut sheets: Vec<(String, String)> = Vec::new();
    let mut reader = Reader::from_reader(workbook.as_slice());
    loop {
        match reader.read_event() {
            Ok(Event::Start(e) | Event::Empty(e)) if e.local_name().as_ref() == b"sheet" => {
                if let (Some(name), Some(id)) = (attr(&e, "name"), attr(&e, "r:id").or_else(|| attr(&e, "id"))) {
                    sheets.push((name, id));
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(bad(format!("the spreadsheet is not valid: {error}"))),
        }
    }
    let wanted = match sheet {
        Some(name) => sheets
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .ok_or_else(|| bad(format!("the spreadsheet has no sheet called `{name}`")))?,
        None => sheets.first().ok_or_else(|| bad("the spreadsheet has no sheets"))?,
    };
    let mut target = None;
    let mut reader = Reader::from_reader(rels.as_slice());
    loop {
        match reader.read_event() {
            Ok(Event::Start(e) | Event::Empty(e)) if e.local_name().as_ref() == b"Relationship" => {
                if attr(&e, "Id").as_deref() == Some(wanted.1.as_str()) {
                    target = attr(&e, "Target");
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(bad(format!("the spreadsheet is not valid: {error}"))),
        }
    }
    let path = match target {
        Some(t) if t.starts_with('/') => t.trim_start_matches('/').to_string(),
        Some(t) => format!("xl/{t}"),
        None => "xl/worksheets/sheet1.xml".to_string(),
    };
    let shared = match entry(&mut archive, "xl/sharedStrings.xml", &mut budget)? {
        Some(xml) => texts(&xml, b"si")?,
        None => Vec::new(),
    };
    let sheet_xml = entry(&mut archive, &path, &mut budget)?.ok_or_else(|| bad("the sheet is missing from the spreadsheet"))?;

    let mut rows: Vec<Row> = Vec::new();
    let mut reader = Reader::from_reader(sheet_xml.as_slice());
    let (mut row, mut row_number): (Vec<String>, usize) = (Vec::new(), 0);
    let (mut kind, mut column, mut in_value, mut in_inline) = (String::new(), 0usize, false, false);
    let (mut value, mut inline) = (String::new(), String::new());
    let mut in_cell = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match e.local_name().as_ref() {
                b"row" => {
                    row = Vec::new();
                    row_number = attr(&e, "r").and_then(|r| r.parse().ok()).unwrap_or(rows.last().map_or(1, |r| r.number + 1));
                }
                b"c" => {
                    in_cell = true;
                    kind = attr(&e, "t").unwrap_or_default();
                    column = attr(&e, "r").map_or(row.len(), |r| column_index(&r));
                    value.clear();
                    inline.clear();
                }
                b"v" if in_cell => in_value = true,
                b"is" if in_cell => in_inline = true,
                _ => {}
            },
            Ok(Event::Empty(e)) => {
                if e.local_name().as_ref() == b"c" {
                    // An empty cell: nothing to add, the next cell's reference leaves the gap.
                } else if e.local_name().as_ref() == b"row" {
                    // An empty row: nothing.
                }
            }
            Ok(Event::Text(t)) if in_value => value.push_str(&t.decode().map_err(|e| bad(e.to_string()))?),
            Ok(Event::GeneralRef(r)) if in_value => value.push(entity(&r)?),
            Ok(Event::Text(t)) if in_inline => inline.push_str(&t.decode().map_err(|e| bad(e.to_string()))?),
            Ok(Event::GeneralRef(r)) if in_inline => inline.push(entity(&r)?),
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"v" => in_value = false,
                b"is" => in_inline = false,
                b"c" => {
                    in_cell = false;
                    let text = match kind.as_str() {
                        "s" => value.trim().parse::<usize>().ok().and_then(|i| shared.get(i)).cloned().unwrap_or_default(),
                        "inlineStr" => inline.clone(),
                        "b" => if value.trim() == "1" { "TRUE".to_string() } else { "FALSE".to_string() },
                        _ => value.clone(),
                    };
                    if column >= limits.max_columns {
                        return Err(TableError::TooManyColumns(limits.max_columns));
                    }
                    if row.len() <= column {
                        row.resize(column + 1, String::new());
                    }
                    row[column] = text;
                }
                b"row" => {
                    rows.push(Row {
                        number: row_number,
                        cells: std::mem::take(&mut row),
                    });
                    if rows.len() > limits.max_rows + 1 {
                        return Err(TableError::TooManyRows(limits.max_rows));
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(bad(format!("the spreadsheet is not valid: {error}"))),
        }
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn read(text: &str) -> Result<Table, TableError> {
        parse(text.as_bytes(), None, Limits::default())
    }

    #[test]
    fn a_csv_is_headers_and_rows_with_their_numbers() {
        let table = read("name,age\nAnn,30\nBob,41\n").expect("a table");
        assert_eq!(table.headers, vec!["name", "age"]);
        assert_eq!(table.rows.len(), 2);
        assert_eq!((table.rows[0].number, table.rows[0].cells.clone()), (2, vec!["Ann".to_string(), "30".to_string()]));
        assert_eq!(table.rows[1].number, 3);
        assert_eq!(table.format, Format::Csv { delimiter: ',' });
    }

    #[test]
    fn quotes_commas_newlines_and_doubled_quotes_inside_a_value() {
        let table = read("a,b\n\"x, y\",\"line one\nline two\"\n\"say \"\"hi\"\"\",plain\n").expect("a table");
        assert_eq!(table.rows[0].cells, vec!["x, y", "line one\nline two"]);
        assert_eq!(table.rows[1].cells, vec!["say \"hi\"", "plain"]);
        assert_eq!(read("a,b\n\"never closed,1\n"), Err(TableError::Bad("a quoted value is never closed".into())));
    }

    #[test]
    fn the_delimiter_is_found_and_other_line_endings_work() {
        assert_eq!(read("a;b;c\n1;2;3\n").expect("t").format, Format::Csv { delimiter: ';' });
        assert_eq!(read("a\tb\n1\t2\n").expect("t").format, Format::Csv { delimiter: '\t' });
        assert_eq!(read("a|b\n1|2\n").expect("t").format, Format::Csv { delimiter: '|' });
        assert_eq!(read("a,\"b;c;d\"\n1,2\n").expect("t").format, Format::Csv { delimiter: ',' }, "separators inside quotes do not count");
        for text in ["a,b\r\n1,2\r\n", "a,b\r1,2\r", "a,b\n1,2"] {
            let table = read(text).expect("a table");
            assert_eq!((table.headers.clone(), table.rows[0].cells.clone()), (vec!["a".to_string(), "b".to_string()], vec!["1".to_string(), "2".to_string()]), "{text:?}");
        }
    }

    #[test]
    fn a_byte_order_mark_and_the_old_code_page_are_handled() {
        let mut with_bom = vec![0xEF, 0xBB, 0xBF];
        with_bom.extend_from_slice(b"name\nAnn\n");
        assert_eq!(parse(&with_bom, None, Limits::default()).expect("t").headers, vec!["name"]);
        // "Zoë" and a euro sign in Windows-1252.
        let latin = [b'n', b'a', b'm', b'e', b'\n', b'Z', b'o', 0xEB, b' ', 0x80, b'\n'];
        let table = parse(&latin, None, Limits::default()).expect("t");
        assert_eq!(table.rows[0].cells, vec!["Zoë €"]);
    }

    #[test]
    fn empty_rows_go_short_rows_are_padded_and_headers_are_named() {
        let table = read("a,,c\n1,2\n\n   ,  ,\n4,5,6,7\n").expect("t");
        assert_eq!(table.headers, vec!["a", "column 2", "c"]);
        assert_eq!(table.rows.len(), 2, "the blank rows are gone");
        assert_eq!(table.rows[0].cells, vec!["1", "2", ""], "padded");
        assert_eq!(table.rows[1].cells, vec!["4", "5", "6"], "extra cells dropped");
        assert_eq!(table.rows[1].number, 5, "numbers are the file's, blank rows included");
        assert_eq!(read(""), Err(TableError::Empty));
        assert_eq!(read("\n\n,,\n"), Err(TableError::Empty));
    }

    #[test]
    fn limits_stop_a_file_that_is_too_big() {
        let small = Limits { max_rows: 2, max_columns: 3, max_unpacked: 1024 };
        assert!(parse(b"a,b\n1,2\n3,4\n", None, small).is_ok());
        assert_eq!(parse(b"a,b\n1,2\n3,4\n5,6\n", None, small), Err(TableError::TooManyRows(2)));
        assert_eq!(parse(b"a,b,c,d\n1,2,3,4\n", None, small), Err(TableError::TooManyColumns(3)));
    }

    /// A minimal XLSX made by hand: shared strings, inline strings, numbers, booleans, a gap and a second sheet.
    fn xlsx() -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut out);
            let options = zip::write::SimpleFileOptions::default();
            let mut add = |name: &str, body: &str| {
                zip.start_file(name, options).expect("start");
                zip.write_all(body.as_bytes()).expect("write");
            };
            add("xl/workbook.xml", r#"<workbook xmlns:r="r"><sheets><sheet name="Leads" sheetId="1" r:id="rId1"/><sheet name="Other" sheetId="2" r:id="rId2"/></sheets></workbook>"#);
            add("xl/_rels/workbook.xml.rels", r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Target="worksheets/sheet2.xml"/></Relationships>"#);
            add("xl/sharedStrings.xml", r#"<sst><si><t>name</t></si><si><t>amount</t></si><si><t>Ann &amp; Co</t></si><si><r><t>Bo</t></r><r><t>b</t></r></si><si><t>ok</t></si></sst>"#);
            add("xl/worksheets/sheet1.xml", r#"<worksheet><sheetData>
                <row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c><c r="D1" t="inlineStr"><is><t>flag</t></is></c></row>
                <row r="2"><c r="A2" t="s"><v>2</v></c><c r="B2"><v>10.5</v></c><c r="D2" t="b"><v>1</v></c></row>
                <row r="4"><c r="A4" t="s"><v>3</v></c><c r="B4"><v>45000</v></c></row>
                </sheetData></worksheet>"#);
            add("xl/worksheets/sheet2.xml", r#"<worksheet><sheetData><row r="1"><c r="A1" t="s"><v>4</v></c></row><row r="2"><c r="A2" t="str"><v>second</v></c></row></sheetData></worksheet>"#);
            zip.finish().expect("finish");
        }
        out.into_inner()
    }

    #[test]
    fn an_xlsx_is_read_with_shared_strings_gaps_and_the_row_numbers_of_the_sheet() {
        let table = parse(&xlsx(), None, Limits::default()).expect("a table");
        assert_eq!(table.format, Format::Xlsx);
        assert_eq!(table.headers, vec!["name", "amount", "column 3", "flag"], "an empty column keeps its place");
        assert_eq!(table.rows[0].cells, vec!["Ann & Co", "10.5", "", "TRUE"], "an entity is resolved, a boolean is text");
        assert_eq!(table.rows[1].cells, vec!["Bob", "45000", "", ""], "rich text is joined");
        assert_eq!((table.rows[0].number, table.rows[1].number), (2, 4), "the empty row 3 is a gap in the numbers");
    }

    #[test]
    fn a_sheet_can_be_chosen_by_name() {
        let table = parse(&xlsx(), Some("other"), Limits::default()).expect("a table");
        assert_eq!(table.headers, vec!["ok"]);
        assert_eq!(table.rows[0].cells, vec!["second"]);
        assert!(matches!(parse(&xlsx(), Some("nope"), Limits::default()), Err(TableError::Bad(m)) if m.contains("no sheet")));
    }

    #[test]
    fn a_zip_that_is_not_a_spreadsheet_or_unpacks_too_big_is_refused() {
        assert!(matches!(parse(b"PK\x03\x04junk", None, Limits::default()), Err(TableError::Bad(_))));
        let tight = Limits { max_unpacked: 100, ..Limits::default() };
        assert!(matches!(parse(&xlsx(), None, tight), Err(TableError::Bad(m)) if m.contains("limit")));
    }

    #[test]
    fn column_letters_are_numbers() {
        assert_eq!([column_index("A1"), column_index("Z9"), column_index("AA1"), column_index("AB12")], [0, 25, 26, 27]);
    }
}
