use std::{
    error::Error,
    fs,
    io::{self, Read},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs},
    path::Path,
    sync::mpsc,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{
    Url,
    blocking::{Client, Response},
    header::{CONTENT_LENGTH, CONTENT_TYPE, ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED},
    redirect::Policy,
};
use sha2::{Digest, Sha256};

pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

pub const MAX_SOURCE_BYTES: usize = 5 * 1024 * 1024;
const SEGMENT_CHARS: usize = 1_500;
const DNS_TIMEOUT: Duration = Duration::from_secs(3);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug)]
pub struct PreparedDocument {
    pub uri: String,
    pub title: Option<String>,
    pub media_type: String,
    pub content_hash: String,
    pub raw_content: Vec<u8>,
    pub extracted_text: String,
    pub published_at: Option<String>,
    pub last_modified_at: Option<String>,
    pub etag: Option<String>,
    pub retrieved_at: i64,
}

#[derive(Debug)]
pub enum FetchResult {
    Modified(PreparedDocument),
    NotModified { uri: String, retrieved_at: i64 },
}

pub fn prepare_local(path: &Path, allow_root: &Path) -> Result<PreparedDocument> {
    let root = allow_root.canonicalize()?;
    let canonical_path = path.canonicalize()?;
    if !is_within(&root, &canonical_path) {
        return Err(invalid_input(format!(
            "local source is outside the permitted root: {}",
            canonical_path.display()
        )));
    }
    let metadata = fs::metadata(&canonical_path)?;
    if !metadata.is_file() {
        return Err(invalid_input("local source must be a regular file"));
    }
    if metadata.len() > MAX_SOURCE_BYTES as u64 {
        return Err(invalid_input(format!(
            "source exceeds the {} MiB limit",
            MAX_SOURCE_BYTES / (1024 * 1024)
        )));
    }
    let raw_content = fs::read(&canonical_path)?;
    if raw_content.len() > MAX_SOURCE_BYTES {
        return Err(invalid_input(format!(
            "source exceeds the {} MiB limit",
            MAX_SOURCE_BYTES / (1024 * 1024)
        )));
    }
    let media_type = media_type_for_path(&canonical_path)?;
    let (extracted_text, title, published_at) = extract(&raw_content, &media_type)?;
    let uri = Url::from_file_path(&canonical_path)
        .map_err(|_| invalid_input("local source path cannot be represented as a file URL"))?
        .to_string();
    Ok(PreparedDocument {
        uri,
        title: title.or_else(|| {
            canonical_path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        }),
        media_type,
        content_hash: hash_bytes(&raw_content),
        raw_content,
        extracted_text,
        published_at,
        last_modified_at: None,
        etag: None,
        retrieved_at: now_unix(),
    })
}

pub fn fetch_url(
    input: &str,
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> Result<FetchResult> {
    let mut url = Url::parse(input)?;
    validate_source_url(&url)?;
    let host = url
        .host_str()
        .ok_or_else(|| invalid_input("source URL must include a hostname"))?
        .to_owned();
    url.set_fragment(None);
    let uri = url.to_string();
    let addresses = resolve_public_addresses(host.clone(), 443)?;
    let client = Client::builder()
        .https_only(true)
        .redirect(Policy::none())
        .no_proxy()
        .connect_timeout(Duration::from_secs(4))
        .timeout(REQUEST_TIMEOUT)
        .resolve_to_addrs(&host, &addresses)
        .user_agent(concat!("Viberaven/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let mut request = client.get(url.clone());
    if let Some(value) = etag {
        request = request.header(IF_NONE_MATCH, value);
    }
    if let Some(value) = last_modified {
        request = request.header(IF_MODIFIED_SINCE, value);
    }
    let response = request
        .send()
        .map_err(|error| Box::new(error.without_url()) as Box<dyn Error + Send + Sync>)?;
    let retrieved_at = now_unix();
    if response.status() == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(FetchResult::NotModified { uri, retrieved_at });
    }
    if response.status().is_redirection() {
        return Err(invalid_input("redirects are disabled for source retrieval"));
    }
    let document = prepare_response(uri, response, retrieved_at)?;
    Ok(FetchResult::Modified(document))
}

pub fn safe_source_uri(input: &str) -> Option<String> {
    let mut url = Url::parse(input).ok()?;
    validate_source_url(&url).ok()?;
    url.set_fragment(None);
    Some(url.to_string())
}

fn validate_source_url(url: &Url) -> Result<()> {
    if url.scheme() != "https" {
        return Err(invalid_input("only HTTPS source URLs are supported"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid_input("source URLs must not include credentials"));
    }
    if url.port_or_known_default() != Some(443) {
        return Err(invalid_input("source URLs must use the default HTTPS port"));
    }
    if url
        .query_pairs()
        .any(|(key, _)| is_credential_query_key(&key))
    {
        return Err(invalid_input(
            "source URLs must not use credential-like query parameters",
        ));
    }
    Ok(())
}

fn is_credential_query_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase().replace('-', "_");
    matches!(
        key.as_str(),
        "token"
            | "access_token"
            | "refresh_token"
            | "api_key"
            | "apikey"
            | "key"
            | "secret"
            | "client_secret"
            | "signature"
            | "sig"
            | "password"
            | "auth"
            | "authorization"
            | "code"
            | "session"
            | "jwt"
    ) || key.ends_with("_token")
        || key.ends_with("_secret")
        || key.starts_with("x_amz_")
}

pub fn hash_bytes(content: &[u8]) -> String {
    let digest = Sha256::digest(content);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .min(i64::MAX as u64) as i64
}

pub fn split_evidence(text: &str) -> Vec<String> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut segments = Vec::new();
    for paragraph in normalized.split("\n\n") {
        let paragraph = paragraph.trim();
        if paragraph.is_empty() {
            continue;
        }
        let chars: Vec<char> = paragraph.chars().collect();
        let mut start = 0;
        while start < chars.len() {
            let mut end = (start + SEGMENT_CHARS).min(chars.len());
            if end < chars.len()
                && let Some(space) = (start..end)
                    .rev()
                    .find(|index| chars[*index].is_whitespace())
                && space > start
            {
                end = space;
            }
            let segment = chars[start..end].iter().collect::<String>();
            let segment = segment.trim();
            if !segment.is_empty() {
                segments.push(segment.to_owned());
            }
            start = end;
            while start < chars.len() && chars[start].is_whitespace() {
                start += 1;
            }
        }
    }
    segments
}

fn prepare_response(
    uri: String,
    mut response: Response,
    retrieved_at: i64,
) -> Result<PreparedDocument> {
    if !response.status().is_success() {
        return Err(invalid_input(format!(
            "source returned HTTP status {}",
            response.status()
        )));
    }
    if response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > MAX_SOURCE_BYTES as u64)
    {
        return Err(invalid_input("source response exceeds the 5 MiB limit"));
    }
    let media_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .unwrap_or("text/html")
        .trim()
        .to_ascii_lowercase();
    if !matches!(
        media_type.as_str(),
        "text/html" | "application/xhtml+xml" | "text/plain" | "text/markdown"
    ) {
        return Err(invalid_input(format!(
            "unsupported source media type: {media_type}"
        )));
    }
    let etag = header_value(&response, &ETAG);
    let last_modified_at = header_value(&response, &LAST_MODIFIED);
    let mut raw_content = Vec::new();
    response
        .by_ref()
        .take(MAX_SOURCE_BYTES as u64 + 1)
        .read_to_end(&mut raw_content)?;
    if raw_content.len() > MAX_SOURCE_BYTES {
        return Err(invalid_input("source response exceeds the 5 MiB limit"));
    }
    let (extracted_text, title, published_at) = extract(&raw_content, &media_type)?;
    if extracted_text.trim().is_empty() {
        return Err(invalid_input("source contains no extractable text"));
    }
    Ok(PreparedDocument {
        uri,
        title,
        media_type,
        content_hash: hash_bytes(&raw_content),
        raw_content,
        extracted_text,
        published_at,
        last_modified_at,
        etag,
        retrieved_at,
    })
}

fn media_type_for_path(path: &Path) -> Result<String> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("md" | "markdown" | "txt" | "rst") => Ok("text/plain".to_owned()),
        Some("html" | "htm" | "xhtml") => Ok("text/html".to_owned()),
        Some(extension) => Err(invalid_input(format!(
            "unsupported local document extension: .{extension}"
        ))),
        None => Err(invalid_input(
            "local source needs a .txt, .md, .rst, or .html extension",
        )),
    }
}

fn extract(
    raw_content: &[u8],
    media_type: &str,
) -> Result<(String, Option<String>, Option<String>)> {
    let text = std::str::from_utf8(raw_content)
        .map_err(|_| invalid_input("source is not valid UTF-8 text"))?
        .trim_start_matches('\u{feff}');
    let (rendered, title, published_at) =
        if matches!(media_type, "text/html" | "application/xhtml+xml") {
            let rendered = html2text::from_read(raw_content, 100)?;
            (rendered, html_title(text), html_published_at(text))
        } else {
            (text.to_owned(), None, None)
        };
    let rendered = rendered
        .replace('\u{00a0}', " ")
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    Ok((rendered.trim().to_owned(), title, published_at))
}

fn html_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title")?;
    let content_start = lower[start..].find('>')? + start + 1;
    let content_end = lower[content_start..].find("</title>")? + content_start;
    let title = html[content_start..content_end]
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .trim()
        .to_owned();
    (!title.is_empty()).then_some(title)
}

fn html_published_at(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let mut offset = 0;
    while let Some(found) = lower[offset..].find("<meta") {
        let tag_start = offset + found;
        let tag_end = lower[tag_start..].find('>')? + tag_start;
        let tag = &html[tag_start..=tag_end];
        let names = [
            attribute_value(tag, "property"),
            attribute_value(tag, "name"),
            attribute_value(tag, "itemprop"),
        ];
        if names.iter().flatten().any(|name| {
            matches!(
                name.to_ascii_lowercase().as_str(),
                "article:published_time" | "datepublished" | "date"
            )
        }) {
            return attribute_value(tag, "content");
        }
        offset = tag_end + 1;
        if offset >= lower.len() {
            break;
        }
    }
    None
}

fn attribute_value(tag: &str, attribute: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let needle = format!("{attribute}=");
    let start = lower.find(&needle)? + needle.len();
    let quote = tag.as_bytes().get(start).copied()?;
    if quote == b'\'' || quote == b'"' {
        let value_start = start + 1;
        let end = tag[value_start..].find(char::from(quote))? + value_start;
        Some(tag[value_start..end].trim().to_owned())
    } else {
        let end = tag[start..]
            .find(|character: char| character.is_whitespace() || character == '>')?
            + start;
        Some(tag[start..end].trim().to_owned())
    }
}

fn header_value(response: &Response, name: &reqwest::header::HeaderName) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn resolve_public_addresses(host: String, port: u16) -> Result<Vec<SocketAddr>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = if let Ok(address) = host.parse::<IpAddr>() {
            Ok(vec![SocketAddr::new(address, port)])
        } else {
            (host.as_str(), port)
                .to_socket_addrs()
                .map(|addresses| addresses.collect::<Vec<_>>())
        };
        let _ = sender.send(result);
    });
    let addresses = receiver
        .recv_timeout(DNS_TIMEOUT)
        .map_err(|_| invalid_input("source hostname resolution timed out"))??;
    if addresses.is_empty() || addresses.iter().any(|address| !is_public_ip(address.ip())) {
        return Err(invalid_input(
            "source hostname resolved to an empty or non-public address set",
        ));
    }
    Ok(addresses)
}

fn is_public_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_public_v4(address),
        IpAddr::V6(address) => is_public_v6(address),
    }
}

fn is_public_v4(address: Ipv4Addr) -> bool {
    let [a, b, c, d] = address.octets();
    !(address.is_private()
        || address.is_loopback()
        || address.is_link_local()
        || address.is_broadcast()
        || address.is_unspecified()
        || address.is_multicast()
        || a == 0
        || a >= 224
        || (a == 100 && (64..=127).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 0 && c == 2)
        || (a == 192 && b == 88 && c == 99)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || (a == 255 && b == 255 && c == 255 && d == 255))
}

fn is_public_v6(address: Ipv6Addr) -> bool {
    let segments = address.segments();
    let global_unicast = segments[0] & 0xe000 == 0x2000;
    global_unicast
        && !address.is_loopback()
        && !address.is_unspecified()
        && !address.is_multicast()
        && !(segments[0] == 0x2001 && segments[1] <= 0x01ff)
        && !(segments[0] == 0x2001 && segments[1] == 0x0db8)
        && segments[0] != 0x2002
        && segments[0] != 0x3fff
}

fn is_within(root: &Path, candidate: &Path) -> bool {
    candidate.starts_with(root)
}

fn invalid_input(message: impl Into<String>) -> Box<dyn Error + Send + Sync> {
    Box::new(io::Error::new(io::ErrorKind::InvalidInput, message.into()))
}

#[cfg(test)]
mod tests {
    use super::{is_public_ip, safe_source_uri};
    use std::net::IpAddr;

    #[test]
    fn source_uri_canonicalization_keeps_public_query_values_but_rejects_credentials() {
        assert_eq!(
            safe_source_uri("https://example.com/article?lang=en#section").as_deref(),
            Some("https://example.com/article?lang=en")
        );
        assert!(safe_source_uri("https://example.com/article?access_token=secret").is_none());
        assert!(safe_source_uri("https://user:pass@example.com/article").is_none());
        assert!(safe_source_uri("http://example.com/article").is_none());
    }

    #[test]
    fn source_resolution_rejects_non_public_address_ranges() {
        for value in [
            "0.0.0.0",
            "10.0.0.1",
            "100.64.0.1",
            "127.0.0.1",
            "169.254.1.1",
            "192.0.2.1",
            "198.18.0.1",
            "224.0.0.1",
            "::1",
            "fc00::1",
            "fe80::1",
            "2001:db8::1",
            "2002::1",
            "3fff::1",
        ] {
            let address = value.parse::<IpAddr>().unwrap();
            assert!(!is_public_ip(address), "{value}");
        }
        assert!(is_public_ip("8.8.8.8".parse().unwrap()));
        assert!(is_public_ip("2606:4700:4700::1111".parse().unwrap()));
    }
}
