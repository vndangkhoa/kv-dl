//! URL normalization: accepts normal YouTube links *and* the "domain-swap"
//! mirror form (`youtube.<host-domain>` ⇄ `youtube.com`), for any hosting
//! domain — the swap suffix is derived from the request's Host header.

use form_urlencoded::Serializer;

use url::Url;

const ALLOWED_HOSTS: [&str; 5] = [
    "youtube.com",
    "www.youtube.com",
    "m.youtube.com",
    "music.youtube.com",
    "youtu.be",
];

const DROP_PARAMS: [&str; 7] = ["list", "index", "start_radio", "si", "pp", "app", "feature"];

/// Suffix of the reference deployment, kept so links in the wild keep working
/// even when this binary runs somewhere else (e.g. local development).
const FALLBACK_SUFFIX: &str = "vndns.net";

/// Map a mirror host like `youtube.example.net` back to `youtube.com`, using
/// the domain this instance is served on (`self_host`) or the fallback.
fn unmirror(host: &str, self_host: Option<&str>) -> Option<String> {
    let mut suffixes: Vec<&str> = Vec::new();
    if let Some(s) = self_host {
        if !s.is_empty() {
            suffixes.push(s);
        }
    }
    suffixes.push(FALLBACK_SUFFIX);

    for suffix in suffixes {
        if host == suffix {
            return Some("youtube.com".to_string());
        }
        if host == format!("youtu.{suffix}") {
            return Some("youtu.be".to_string());
        }
        if let Some(base) = host.strip_suffix(&format!(".{suffix}")) {
            if !base.is_empty() && base != "youtu" {
                return Some(format!("{base}.com"));
            }
        }
    }
    None
}

pub fn normalize_url(raw: &str, self_host: Option<&str>) -> Result<String, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("Empty URL".into());
    }
    let with_scheme = if raw.contains("://") {
        raw.to_string()
    } else {
        format!("https://{raw}")
    };

    let mut u = Url::parse(&with_scheme).map_err(|e| format!("Could not parse URL: {e}"))?;
    let host = u
        .host_str()
        .ok_or_else(|| "Could not parse host from URL".to_string())?
        .to_lowercase();

    // Undo the ".com" -> mirror-domain swap (any hosting domain).
    let host = match unmirror(&host, self_host) {
        Some(h) => h,
        None => host,
    };

    if !ALLOWED_HOSTS.contains(&host.as_str()) {
        return Err(format!("Unsupported host: {host}"));
    }

    // Drop playlist/radio params so a single video is fetched.
    let mut ser = Serializer::new(String::new());
    for (k, v) in u.query().map(|q| form_urlencoded::parse(q.as_bytes())).into_iter().flatten() {
        if !DROP_PARAMS.contains(&k.to_lowercase().as_str()) {
            ser.append_pair(&k, &v);
        }
    }
    let query = ser.finish();
    let query = if query.is_empty() { None } else { Some(query.as_str()) };

    u.set_fragment(None);
    u.set_query(query);
    let path = {
        let p = u.path();
        if p.is_empty() { "/".to_string() } else { p.to_string() }
    };
    u.set_path(&path);
    u.set_host(Some(&host)).map_err(|e| e.to_string())?;
    let _ = u.set_scheme("https");
    u.set_port(None).ok();

    Ok(u.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_youtube_links_pass_through() {
        assert_eq!(
            normalize_url("https://www.youtube.com/watch?v=abc&t=4s", None).unwrap(),
            "https://www.youtube.com/watch?v=abc&t=4s"
        );
    }

    #[test]
    fn fallback_suffix_still_works() {
        assert_eq!(
            normalize_url("https://youtube.vndns.net/watch?v=abc&list=x", None).unwrap(),
            "https://youtube.com/watch?v=abc"
        );
        assert_eq!(
            normalize_url("https://youtu.vndns.net/abc", None).unwrap(),
            "https://youtu.be/abc"
        );
    }

    #[test]
    fn self_hosted_suffix_is_derived_from_request_host() {
        assert_eq!(
            normalize_url(
                "https://youtube.dl.example.net/watch?v=abc&list=x",
                Some("dl.example.net")
            )
            .unwrap(),
            "https://youtube.com/watch?v=abc"
        );
        assert_eq!(
            normalize_url("https://youtu.dl.example.net/abc", Some("dl.example.net")).unwrap(),
            "https://youtu.be/abc"
        );
        assert_eq!(
            normalize_url("https://dl.example.net/watch?v=abc", Some("dl.example.net")).unwrap(),
            "https://youtube.com/watch?v=abc"
        );
    }

    #[test]
    fn unknown_host_is_rejected() {
        assert!(normalize_url(
            "https://evil.example.com/watch?v=abc",
            Some("dl.example.net")
        )
        .is_err());
    }
}
