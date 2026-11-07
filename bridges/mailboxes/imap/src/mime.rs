//! Reading a mail message (RFC 5322 and the MIME parts of RFC 2045 to 2047): headers, addresses, the text and the attachments.
//! Enough for the mail people receive; what it cannot read it shows as it is, never as an error.

use base64::{Engine, engine::general_purpose::STANDARD};

/// One address of a header: a name (may be empty) and the address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone)]
pub struct Attachment {
    pub name: String,
    pub kind: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Default)]
pub struct Parsed {
    pub headers: Vec<(String, String)>,
    pub text: String,
    pub attachments: Vec<Attachment>,
}

impl Parsed {
    /// The first header called `name` (any case), with encoded words decoded.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
    pub fn addresses(&self, name: &str) -> Vec<Address> {
        self.header(name).map(addresses).unwrap_or_default()
    }
    pub fn subject(&self) -> String {
        self.header("subject").map(decode_words).unwrap_or_default()
    }
}

/// Most nested parts followed: a message built to exhaust the reader gets its deepest parts left out.
const MAX_DEPTH: usize = 12;
/// Most attachments listed from one message.
const MAX_PARTS: usize = 100;

pub fn parse(raw: &[u8]) -> Parsed {
    let (head, body) = split(raw);
    let headers = headers_of(head);
    let mut out = Parsed::default();
    let (mut plain, mut html) = (Vec::new(), Vec::new());
    walk(&headers, body, 0, &mut plain, &mut html, &mut out.attachments);
    let joined = |parts: &[String]| parts.iter().map(|p| p.trim_end().to_string()).collect::<Vec<_>>().join("\n\n");
    out.text = if !plain.is_empty() { joined(&plain) } else { jmap::strip_html(&joined(&html)) };
    out.headers = headers;
    out
}

/// Head and body of a message or a part: split at the first empty line.
fn split(raw: &[u8]) -> (&[u8], &[u8]) {
    let mut i = 0;
    while i < raw.len() {
        if raw[i..].starts_with(b"\r\n\r\n") {
            return (&raw[..i], &raw[i + 4..]);
        }
        if raw[i..].starts_with(b"\n\n") {
            return (&raw[..i], &raw[i + 2..]);
        }
        i += 1;
    }
    (raw, &[])
}

/// The header lines, unfolded, with names kept as written and values as raw text.
fn headers_of(head: &[u8]) -> Vec<(String, String)> {
    let text = String::from_utf8_lossy(head);
    let mut out: Vec<(String, String)> = Vec::new();
    for line in text.split('\n') {
        let line = line.trim_end_matches('\r');
        if line.starts_with([' ', '\t']) {
            if let Some(last) = out.last_mut() {
                last.1.push(' ');
                last.1.push_str(line.trim());
            }
        } else if let Some((name, value)) = line.split_once(':') {
            out.push((name.trim().to_string(), value.trim().to_string()));
        }
    }
    out
}

fn find<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

/// `type/subtype; a=b; c="d"` as the type (lower case) and its parameters (names lower case, `name*=` forms decoded).
pub fn params(value: &str) -> (String, Vec<(String, String)>) {
    let mut parts = split_unquoted(value, ';').into_iter();
    let kind = parts.next().unwrap_or_default().trim().to_ascii_lowercase();
    let mut out = Vec::new();
    for p in parts {
        let Some((k, v)) = p.split_once('=') else { continue };
        let (k, v) = (k.trim().to_ascii_lowercase(), unquote(v.trim()));
        if let Some(base) = k.strip_suffix('*') {
            // RFC 2231: charset'language'percent-encoded
            let mut it = v.splitn(3, '\'');
            let (charset, _, data) = (it.next().unwrap_or("utf-8"), it.next(), it.next().unwrap_or_default());
            let bytes = percent_decode(data);
            out.push((base.to_string(), charset_text(charset, &bytes)));
        } else {
            out.push((k, decode_words(&v)));
        }
    }
    (kind, out)
}

fn param<'a>(list: &'a [(String, String)], name: &str) -> Option<&'a str> {
    list.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

fn percent_decode(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let (mut out, mut i) = (Vec::new(), 0);
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

/// Split at `sep` where it is not inside double quotes (or, for `,`, angle brackets).
fn split_unquoted(s: &str, sep: char) -> Vec<String> {
    let (mut out, mut cur, mut quoted, mut angle, mut escaped) = (Vec::new(), String::new(), false, false, false);
    for c in s.chars() {
        if escaped {
            cur.push(c);
            escaped = false;
            continue;
        }
        match c {
            '\\' if quoted => {
                cur.push(c);
                escaped = true;
            }
            '"' => {
                quoted = !quoted;
                cur.push(c);
            }
            '<' if !quoted => {
                angle = true;
                cur.push(c);
            }
            '>' if !quoted => {
                angle = false;
                cur.push(c);
            }
            c if c == sep && !quoted && !angle => out.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    out.push(cur);
    out
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        let mut out = String::new();
        let mut escaped = false;
        for c in s[1..s.len() - 1].chars() {
            if !escaped && c == '\\' {
                escaped = true;
            } else {
                out.push(c);
                escaped = false;
            }
        }
        out
    } else {
        s.to_string()
    }
}

fn walk(headers: &[(String, String)], body: &[u8], depth: usize, plain: &mut Vec<String>, html: &mut Vec<String>, files: &mut Vec<Attachment>) {
    let (kind, ctype) = params(find(headers, "content-type").unwrap_or("text/plain"));
    let (disposition, dparams) = params(find(headers, "content-disposition").unwrap_or(""));
    if let (true, Some(boundary)) = (kind.starts_with("multipart/"), param(&ctype, "boundary")) {
        if depth < MAX_DEPTH {
            for part in multipart(body, boundary) {
                let (head, rest) = split(part);
                walk(&headers_of(head), rest, depth + 1, plain, html, files);
            }
        }
        return;
    }
    let data = decode_transfer(find(headers, "content-transfer-encoding").unwrap_or(""), body);
    let name = param(&dparams, "filename").or(param(&ctype, "name")).map(str::to_string);
    let is_text = kind == "text/plain" || kind == "text/html";
    if is_text && disposition != "attachment" && name.is_none() {
        let text = charset_text(param(&ctype, "charset").unwrap_or("utf-8"), &data);
        if kind == "text/html" { html.push(text) } else { plain.push(text) }
        return;
    }
    if files.len() < MAX_PARTS {
        let kind = if kind.is_empty() { "application/octet-stream".to_string() } else { kind };
        let name = name.unwrap_or_else(|| if kind == "message/rfc822" { "message.eml".into() } else { "attachment".into() });
        files.push(Attachment { name: name.replace(['/', '\\', '\r', '\n'], "_"), kind, data });
    }
}

/// The parts between the boundary lines.
fn multipart<'a>(body: &'a [u8], boundary: &str) -> Vec<&'a [u8]> {
    let marker = format!("--{boundary}");
    let marker = marker.as_bytes();
    let (mut parts, mut start, mut i) = (Vec::new(), None::<usize>, 0);
    while i < body.len() {
        let at_line_start = i == 0 || body[i - 1] == b'\n';
        if at_line_start && body[i..].starts_with(marker) {
            let rest = &body[i + marker.len()..];
            let closing = rest.starts_with(b"--");
            if let Some(s) = start {
                // the line break before the boundary belongs to the boundary
                let mut end = i;
                if end > s && body[end - 1] == b'\n' {
                    end -= 1;
                    if end > s && body[end - 1] == b'\r' {
                        end -= 1;
                    }
                }
                parts.push(&body[s..end]);
            }
            if closing {
                return parts;
            }
            let eol = rest.iter().position(|b| *b == b'\n').map(|p| p + 1).unwrap_or(rest.len());
            start = Some(i + marker.len() + eol);
            i += marker.len() + eol;
            continue;
        }
        i += 1;
    }
    if let Some(s) = start {
        parts.push(&body[s..]);
    }
    parts
}

fn decode_transfer(encoding: &str, body: &[u8]) -> Vec<u8> {
    match encoding.trim().to_ascii_lowercase().as_str() {
        "base64" => {
            let clean: Vec<u8> = body.iter().copied().filter(|b| !b.is_ascii_whitespace()).collect();
            STANDARD.decode(clean).unwrap_or_default()
        }
        "quoted-printable" => quoted_printable(body, false),
        _ => body.to_vec(),
    }
}

/// Quoted-printable (RFC 2045); in a header word `_` is a space.
fn quoted_printable(body: &[u8], header: bool) -> Vec<u8> {
    let (mut out, mut i) = (Vec::new(), 0);
    while i < body.len() {
        match body[i] {
            b'=' if i + 1 < body.len() && (body[i + 1] == b'\n' || body[i + 1] == b'\r') => {
                // a soft line break
                i += 1;
                if body[i] == b'\r' {
                    i += 1;
                }
                if i < body.len() && body[i] == b'\n' {
                    i += 1;
                }
            }
            b'=' if i + 2 < body.len() => {
                match std::str::from_utf8(&body[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(v) => {
                        out.push(v);
                        i += 3;
                    }
                    None => {
                        out.push(b'=');
                        i += 1;
                    }
                }
            }
            b'_' if header => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// Bytes in a character set as text. UTF-8, ASCII, Latin-1 and Windows-1252 are read; others are read as UTF-8 with what does not fit replaced.
pub fn charset_text(charset: &str, bytes: &[u8]) -> String {
    match charset.trim().to_ascii_lowercase().as_str() {
        "iso-8859-1" | "latin1" | "iso8859-1" => bytes.iter().map(|b| *b as char).collect(),
        "windows-1252" | "cp1252" | "iso-8859-15" => bytes.iter().map(|b| cp1252(*b)).collect(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

fn cp1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
    ];
    if (0x80..0xa0).contains(&b) { HIGH[(b - 0x80) as usize] } else { b as char }
}

/// A header with its `=?charset?B?…?=` words decoded (RFC 2047).
pub fn decode_words(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    let mut previous_was_word = false;
    while let Some(start) = rest.find("=?") {
        let (before, tail) = rest.split_at(start);
        match decode_word(tail) {
            Some((text, used)) => {
                // white space between two encoded words is not part of the text
                if !(previous_was_word && before.trim().is_empty()) {
                    out.push_str(before);
                }
                out.push_str(&text);
                rest = &tail[used..];
                previous_was_word = true;
            }
            None => {
                out.push_str(before);
                out.push_str("=?");
                rest = &tail[2..];
                previous_was_word = false;
            }
        }
    }
    out.push_str(rest);
    out
}

fn decode_word(s: &str) -> Option<(String, usize)> {
    let inner = s.strip_prefix("=?")?;
    let mut it = inner.splitn(3, '?');
    let (charset, enc, tail) = (it.next()?, it.next()?, it.next()?);
    let end = tail.find("?=")?;
    let data = &tail[..end];
    let charset = charset.split('*').next().unwrap_or(charset);
    let bytes = match enc {
        "B" | "b" => STANDARD.decode(data).ok()?,
        "Q" | "q" => quoted_printable(data.as_bytes(), true),
        _ => return None,
    };
    Some((charset_text(charset, &bytes), 2 + charset.len() + 1 + enc.len() + 1 + end + 2))
}

/// A list of addresses (`Ann <ann@x.test>, "Dan, D" <dan@x.test>, kim@x.test`).
pub fn addresses(value: &str) -> Vec<Address> {
    split_unquoted(value, ',')
        .into_iter()
        .filter_map(|item| {
            let item = item.trim().to_string();
            if item.is_empty() {
                return None;
            }
            if let (Some(l), Some(r)) = (item.rfind('<'), item.rfind('>')) {
                if l < r {
                    let email = item[l + 1..r].trim().to_string();
                    let name = decode_words(&unquote(item[..l].trim()));
                    return Some(Address { name, email });
                }
            }
            Some(Address { name: String::new(), email: item.trim_matches(['<', '>']).to_string() })
        })
        .filter(|a| a.email.contains('@'))
        .collect()
}

/// Message ids of a `Message-ID` or `References` header, without the angle brackets.
pub fn message_ids(value: &str) -> Vec<String> {
    value.split_whitespace().map(|p| p.trim_matches(['<', '>']).to_string()).filter(|p| !p.is_empty()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoded_words_are_decoded() {
        assert_eq!(decode_words("=?UTF-8?B?w4ZibGU=?= pie"), "Æble pie");
        assert_eq!(decode_words("=?iso-8859-1?Q?Caf=E9_cr=E8me?="), "Café crème");
        assert_eq!(decode_words("=?UTF-8?Q?a?= =?UTF-8?Q?b?="), "ab");
        assert_eq!(decode_words("plain =? text"), "plain =? text");
    }

    #[test]
    fn address_lists_keep_names_with_commas() {
        let list = addresses("Ann <ann@x.test>, \"Dan, D\" <dan@x.test>, kim@x.test");
        assert_eq!(list.len(), 3);
        assert_eq!(list[1], Address { name: "Dan, D".into(), email: "dan@x.test".into() });
        assert_eq!(list[2].email, "kim@x.test");
    }

    #[test]
    fn a_plain_message_is_read() {
        let p = parse(b"From: Zed <zed@c.test>\r\nSubject: =?UTF-8?B?w4ZibGU=?=\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nHello\r\n");
        assert_eq!(p.subject(), "Æble");
        assert_eq!(p.text.trim(), "Hello");
        assert_eq!(p.addresses("from")[0].email, "zed@c.test");
    }

    #[test]
    fn a_multipart_message_gives_text_and_attachments() {
        let raw = b"Subject: x\r\nContent-Type: multipart/mixed; boundary=\"B1\"\r\n\r\npreamble\r\n--B1\r\nContent-Type: multipart/alternative; boundary=B2\r\n\r\n--B2\r\nContent-Type: text/plain\r\n\r\nplain text\r\n--B2\r\nContent-Type: text/html\r\n\r\n<p>html</p>\r\n--B2--\r\n--B1\r\nContent-Type: application/pdf; name=\"a.pdf\"\r\nContent-Disposition: attachment; filename=\"a.pdf\"\r\nContent-Transfer-Encoding: base64\r\n\r\nSGVsbG8=\r\n--B1--\r\n";
        let p = parse(raw);
        assert_eq!(p.text.trim(), "plain text");
        assert_eq!(p.attachments.len(), 1);
        assert_eq!((p.attachments[0].name.as_str(), p.attachments[0].data.as_slice()), ("a.pdf", &b"Hello"[..]));
    }

    #[test]
    fn html_only_mail_is_shown_as_text_and_quoted_printable_is_read() {
        let p = parse(b"Content-Type: text/html; charset=utf-8\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\n<p>Caf=C3=A9 and a very long=\r\n line</p><script>x()</script>\r\n");
        assert!(p.text.contains("Café and a very long line") && !p.text.contains("x()"), "{}", p.text);
    }

    #[test]
    fn rfc2231_file_names_are_decoded() {
        let (_, list) = params("attachment; filename*=utf-8''r%C3%A9sum%C3%A9.pdf");
        assert_eq!(list[0], ("filename".to_string(), "résumé.pdf".to_string()));
    }
}
