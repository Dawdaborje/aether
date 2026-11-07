//! A small IMAP client (RFC 3501): one connection per call, the commands the mailbox needs, and a reader for the answers.

use std::{sync::Arc, time::Duration};

use aether_communication::SendError;
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    net::TcpStream,
    time::timeout,
};

/// Longest wait for the server's next line.
const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// Largest piece of data (a message) the client takes from the server in one answer.
pub const MAX_LITERAL: usize = 30 * 1024 * 1024;

pub trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Security {
    /// TLS from the first byte (port 993).
    Tls,
    /// Plain first, then `STARTTLS` (port 143).
    StartTls,
    /// No encryption: only for a server on the same machine or network.
    None,
}

/// A value of an answer: `NIL`, a word, a string (quoted or a literal), or a parenthesised list.
#[derive(Debug, Clone, PartialEq)]
pub enum V {
    Nil,
    Atom(String),
    Str(Vec<u8>),
    List(Vec<V>),
}

impl V {
    /// The value as text (words and strings; a list or NIL is empty).
    pub fn text(&self) -> String {
        match self {
            V::Atom(a) => a.clone(),
            V::Str(s) => String::from_utf8_lossy(s).into_owned(),
            _ => String::new(),
        }
    }
    pub fn bytes(&self) -> &[u8] {
        match self {
            V::Str(s) => s,
            V::Atom(a) => a.as_bytes(),
            _ => &[],
        }
    }
    pub fn list(&self) -> &[V] {
        match self {
            V::List(l) => l,
            _ => &[],
        }
    }
    pub fn is(&self, word: &str) -> bool {
        matches!(self, V::Atom(a) if a.eq_ignore_ascii_case(word))
    }
}

/// The word after `key` in a list of alternating names and values (`UID 5 FLAGS (…)`).
pub fn after<'a>(items: &'a [V], key: &str) -> Option<&'a V> {
    items.iter().position(|v| v.is(key)).and_then(|i| items.get(i + 1))
}

/// Read one value at `*i`.
pub fn value(b: &[u8], i: &mut usize) -> Option<V> {
    while *i < b.len() && b[*i] == b' ' {
        *i += 1;
    }
    let c = *b.get(*i)?;
    match c {
        b'(' => {
            *i += 1;
            let mut items = Vec::new();
            loop {
                while *i < b.len() && b[*i] == b' ' {
                    *i += 1;
                }
                match b.get(*i)? {
                    b')' => {
                        *i += 1;
                        return Some(V::List(items));
                    }
                    _ => items.push(value(b, i)?),
                }
            }
        }
        b'"' => {
            *i += 1;
            let mut out = Vec::new();
            while *i < b.len() {
                match b[*i] {
                    b'\\' if *i + 1 < b.len() => {
                        out.push(b[*i + 1]);
                        *i += 2;
                    }
                    b'"' => {
                        *i += 1;
                        return Some(V::Str(out));
                    }
                    other => {
                        out.push(other);
                        *i += 1;
                    }
                }
            }
            None
        }
        b'{' => {
            let close = b[*i..].iter().position(|x| *x == b'}')? + *i;
            let n: usize = std::str::from_utf8(&b[*i + 1..close]).ok()?.trim_end_matches('+').parse().ok()?;
            let mut start = close + 1;
            if b[start..].starts_with(b"\r\n") {
                start += 2;
            } else if b[start..].starts_with(b"\n") {
                start += 1;
            }
            let data = b.get(start..start + n)?.to_vec();
            *i = start + n;
            Some(V::Str(data))
        }
        _ => {
            let start = *i;
            while *i < b.len() && !matches!(b[*i], b' ' | b'(' | b')' | b'\r' | b'\n') {
                if b[*i] == b'[' {
                    while *i < b.len() && b[*i] != b']' {
                        *i += 1;
                    }
                }
                *i += 1;
            }
            let word = String::from_utf8_lossy(&b[start..(*i).min(b.len())]).into_owned();
            Some(if word.eq_ignore_ascii_case("NIL") { V::Nil } else { V::Atom(word) })
        }
    }
}

/// All values of one answer line, such as `* 5 FETCH (UID 5 FLAGS (\Seen))`.
pub fn values(line: &[u8]) -> Vec<V> {
    let (mut i, mut out) = (0, Vec::new());
    while let Some(v) = value(line, &mut i) {
        out.push(v);
        while i < line.len() && matches!(line[i], b' ' | b'\r' | b'\n') {
            i += 1;
        }
    }
    out
}

pub struct Client {
    io: BufReader<Box<dyn Io>>,
    tag: u32,
}

fn network(what: &str) -> SendError {
    SendError::transient(format!("the mail server could not be reached ({what})"))
}

fn tls_connector() -> Result<tokio_rustls::TlsConnector, SendError> {
    let roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|_| SendError::permanent("TLS is not available"))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(tokio_rustls::TlsConnector::from(Arc::new(config)))
}

async fn upgrade(host: &str, tcp: TcpStream) -> Result<Box<dyn Io>, SendError> {
    let name = rustls::pki_types::ServerName::try_from(host.to_string()).map_err(|_| SendError::permanent(format!("`{host}` is not a server name")))?;
    let tls = timeout(READ_TIMEOUT, tls_connector()?.connect(name, tcp))
        .await
        .map_err(|_| network("timed out"))?
        .map_err(|e| SendError::permanent(format!("the mail server's certificate or encryption was not accepted ({e})")))?;
    Ok(Box::new(tls))
}

impl Client {
    pub async fn connect(host: &str, port: u16, security: Security) -> Result<Self, SendError> {
        let tcp = timeout(READ_TIMEOUT, TcpStream::connect((host, port))).await.map_err(|_| network("timed out"))?.map_err(|_| network("connection refused"))?;
        let io: Box<dyn Io> = match security {
            Security::Tls => upgrade(host, tcp).await?,
            _ => Box::new(tcp),
        };
        let mut client = Client { io: BufReader::new(io), tag: 0 };
        let greeting = client.read_response().await?;
        if !greeting.starts_with(b"* OK") && !greeting.starts_with(b"* PREAUTH") {
            return Err(SendError::permanent("the server did not greet like a mail server; is the port an IMAP port?"));
        }
        if security == Security::StartTls {
            client.run(&[Part::Text("STARTTLS".into())]).await?;
            let Client { io, tag } = client;
            // Nothing is buffered after the server's OK, so the plain stream can be handed to TLS as it is.
            let tls = upgrade_boxed(host, io.into_inner()).await?;
            client = Client { io: BufReader::new(tls), tag };
        }
        Ok(client)
    }

    /// One answer: a line, with the data of each `{n}` literal inside it, up to the end of the logical line.
    async fn read_response(&mut self) -> Result<Vec<u8>, SendError> {
        let mut all = Vec::new();
        loop {
            let mut line = Vec::new();
            let n = timeout(READ_TIMEOUT, self.io.read_until(b'\n', &mut line)).await.map_err(|_| network("timed out"))?.map_err(|_| network("connection lost"))?;
            if n == 0 {
                return Err(network("connection closed"));
            }
            if line.len() > 1_000_000 {
                return Err(SendError::permanent("the mail server sent a line that is too long"));
            }
            let literal = literal_size(&line);
            all.extend_from_slice(&line);
            match literal {
                Some(size) => {
                    if size > MAX_LITERAL {
                        return Err(SendError::permanent("the mail server sent something larger than 30 MB"));
                    }
                    let mut data = vec![0u8; size];
                    timeout(READ_TIMEOUT, self.io.read_exact(&mut data)).await.map_err(|_| network("timed out"))?.map_err(|_| network("connection lost"))?;
                    all.extend_from_slice(&data);
                }
                None => return Ok(all),
            }
        }
    }

    /// Send a command (text and literals) and read until its tagged answer. The untagged answers come back parsed.
    pub async fn run(&mut self, parts: &[Part]) -> Result<Vec<Vec<V>>, SendError> {
        self.tag += 1;
        let tag = format!("a{}", self.tag);
        let mut line = tag.clone().into_bytes();
        line.push(b' ');
        for part in parts {
            match part {
                Part::Text(t) => line.extend_from_slice(t.as_bytes()),
                Part::Lit(data) => {
                    line.extend_from_slice(format!("{{{}}}\r\n", data.len()).as_bytes());
                    self.write(&line).await?;
                    line.clear();
                    // wait for the server's go-ahead
                    loop {
                        let r = self.read_response().await?;
                        if r.starts_with(b"+") {
                            break;
                        }
                        if r.starts_with(tag.as_bytes()) {
                            return Err(refusal(&r, &tag));
                        }
                    }
                    line.extend_from_slice(data);
                }
            }
        }
        line.extend_from_slice(b"\r\n");
        self.write(&line).await?;
        let mut untagged = Vec::new();
        loop {
            let r = self.read_response().await?;
            if r.starts_with(tag.as_bytes()) && r.get(tag.len()) == Some(&b' ') {
                let status = String::from_utf8_lossy(&r[tag.len() + 1..]).to_string();
                return if status.to_ascii_uppercase().starts_with("OK") { Ok(untagged) } else { Err(refusal(&r, &tag)) };
            }
            if r.starts_with(b"* ") {
                untagged.push(values(&r[2..]));
            }
        }
    }

    pub async fn cmd(&mut self, text: &str) -> Result<Vec<Vec<V>>, SendError> {
        self.run(&[Part::Text(text.to_string())]).await
    }

    async fn write(&mut self, data: &[u8]) -> Result<(), SendError> {
        self.io.write_all(data).await.map_err(|_| network("connection lost"))?;
        self.io.flush().await.map_err(|_| network("connection lost"))
    }

    pub async fn login(&mut self, user: &str, secret: &str) -> Result<(), SendError> {
        if user.contains(['\r', '\n', '\0']) || secret.contains(['\r', '\n', '\0']) {
            return Err(SendError::permanent("the login holds a character a mail server cannot take"));
        }
        self.cmd(&format!("LOGIN {} {}", quote(user), quote(secret))).await.map_err(|e| {
            if e.permanent { SendError::permanent("the mail server did not accept your login") } else { e }
        })?;
        Ok(())
    }

    pub async fn logout(mut self) {
        let _ = self.cmd("LOGOUT").await;
    }
}

async fn upgrade_boxed(host: &str, io: Box<dyn Io>) -> Result<Box<dyn Io>, SendError> {
    let name = rustls::pki_types::ServerName::try_from(host.to_string()).map_err(|_| SendError::permanent(format!("`{host}` is not a server name")))?;
    let tls = timeout(READ_TIMEOUT, tls_connector()?.connect(name, io))
        .await
        .map_err(|_| network("timed out"))?
        .map_err(|e| SendError::permanent(format!("the mail server's certificate or encryption was not accepted ({e})")))?;
    Ok(Box::new(tls))
}

pub enum Part {
    Text(String),
    Lit(Vec<u8>),
}

/// `{123}\r\n` at the end of a line: how many bytes of literal follow.
fn literal_size(line: &[u8]) -> Option<usize> {
    let line = line.strip_suffix(b"\n")?;
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    let line = line.strip_suffix(b"}")?;
    let open = line.iter().rposition(|b| *b == b'{')?;
    std::str::from_utf8(&line[open + 1..]).ok()?.trim_end_matches('+').parse().ok()
}

fn refusal(response: &[u8], tag: &str) -> SendError {
    let text = String::from_utf8_lossy(response);
    let text = text.trim().strip_prefix(tag).unwrap_or(&text).trim();
    let why: String = text.split_once(' ').map(|(_, t)| t).unwrap_or(text).chars().filter(|c| !c.is_control()).take(200).collect();
    if text.to_ascii_uppercase().starts_with("NO") || text.to_ascii_uppercase().starts_with("BAD") {
        SendError::permanent(format!("the mail server says: {why}"))
    } else {
        SendError::transient(format!("the mail server says: {why}"))
    }
}

/// A string for a command: quoted, with `\` and `"` escaped.
pub fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_with_flags_and_a_literal_is_read() {
        let line = b"5 FETCH (UID 5 FLAGS (\\Seen \\Flagged) INTERNALDATE \"19-Oct-2026 10:00:00 +0000\" BODY[] {5}\r\nHello)\r\n";
        let v = values(line);
        assert_eq!(v[1], V::Atom("FETCH".into()));
        let items = v[2].list();
        assert_eq!(after(items, "UID").unwrap().text(), "5");
        assert!(after(items, "FLAGS").unwrap().list().iter().any(|f| f.is("\\Seen")));
        assert_eq!(after(items, "BODY[]").unwrap().bytes(), b"Hello");
    }

    #[test]
    fn brackets_in_a_word_hold_spaces_and_parens() {
        let v = values(b"1 FETCH (BODY[HEADER.FIELDS (FROM SUBJECT)] \"x\" UID 1)\r\n");
        assert_eq!(after(v[2].list(), "BODY[HEADER.FIELDS (FROM SUBJECT)]").unwrap().text(), "x");
    }

    #[test]
    fn nil_and_quoted_escapes() {
        let v = values(b"LIST (\\HasNoChildren) NIL \"a \\\"b\\\" c\"\r\n");
        assert_eq!(v[2], V::Nil);
        assert_eq!(v[3].text(), "a \"b\" c");
    }

    #[test]
    fn literal_markers_are_found_at_the_end_of_a_line() {
        assert_eq!(literal_size(b"* 1 FETCH (BODY[] {42}\r\n"), Some(42));
        assert_eq!(literal_size(b"* OK done\r\n"), None);
    }

    #[test]
    fn strings_are_quoted() {
        assert_eq!(quote("a\"b\\c"), "\"a\\\"b\\\\c\"");
    }
}
