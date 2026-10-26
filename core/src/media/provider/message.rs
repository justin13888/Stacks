//! What an upstream failure may say once it leaves the adapter.
//!
//! A failure's text ends up in logs, in the stale-snapshot warning and in
//! metrics-adjacent tooling, so it must never carry a credential. cameo
//! stringifies reqwest's errors, and reqwest names the request URL, which
//! for a TMDB v3 key ends in `?api_key=<key>`. Every message the adapter
//! classifies goes through [`sanitize`]; the requests Stacks sends itself
//! drop the URL before they are stringified ([`transport_kind`]). The cameo
//! side of this gap is tracked as gap 10 of
//! <https://github.com/justin13888/Stacks/issues/14>.

use crate::media::error::{MetadataErrorKind, UnavailableCause};

/// What a redacted secret is replaced with.
const REDACTED: &str = "<redacted>";

/// Query parameters whose values are credentials, compared
/// case-insensitively.
const SECRET_PARAMS: &[&str] = &[
    "api_key",
    "apikey",
    "key",
    "token",
    "access_token",
    "auth",
    "password",
    "secret",
];

/// Redact every credential in `s`, then bound it for logs.
pub(crate) fn sanitize(s: &str) -> String {
    truncate(&redact(s))
}

/// Replace the value of every secret-named query parameter (`?api_key=…`,
/// `&token=…`) and the user information of every URL (`://user:pass@`).
fn redact(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find(['?', '&', '@']) {
        let (head, tail) = rest.split_at(at);
        out.push_str(head);
        let (sep, tail) = tail.split_at(1);
        if sep == "@" {
            // `scheme://userinfo@host`: drop everything after `://`.
            if let Some(scheme_end) = out.rfind("://") {
                let userinfo = &out[scheme_end + 3..];
                let in_authority =
                    !userinfo.contains(|c: char| matches!(c, '/' | '?' | '#') || c.is_whitespace());
                if !userinfo.is_empty() && in_authority {
                    out.truncate(scheme_end + 3);
                    out.push_str(REDACTED);
                }
            }
            out.push('@');
            rest = tail;
            continue;
        }
        out.push_str(sep);
        let name_end = tail.find('=').unwrap_or(0);
        let name = &tail[..name_end];
        let is_secret = name_end > 0
            && SECRET_PARAMS
                .iter()
                .any(|secret| secret.eq_ignore_ascii_case(name));
        if is_secret {
            out.push_str(name);
            out.push('=');
            out.push_str(REDACTED);
            let value = &tail[name_end + 1..];
            let value_end = value
                .find(|c: char| c == '&' || c == '#' || c == ')' || c == '"' || c.is_whitespace())
                .unwrap_or(value.len());
            rest = &value[value_end..];
        } else {
            rest = tail;
        }
    }
    out.push_str(rest);
    out
}

/// Bound an upstream message (which may carry a response body) for logs.
fn truncate(s: &str) -> String {
    const MAX: usize = 512;
    if s.len() <= MAX {
        return s.to_owned();
    }
    let mut end = MAX;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

/// Classify a failure of a request Stacks sent itself. The URL is dropped
/// before the error is stringified, so its query never reaches a log.
pub(crate) fn transport_kind(e: reqwest::Error) -> MetadataErrorKind {
    if e.is_timeout() {
        MetadataErrorKind::Unavailable(UnavailableCause::Timeout)
    } else if e.is_decode() {
        MetadataErrorKind::BadUpstreamResponse(sanitize(&e.without_url().to_string()))
    } else {
        MetadataErrorKind::Unavailable(UnavailableCause::Transport(sanitize(
            &e.without_url().to_string(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_respects_char_boundaries() {
        let s = "é".repeat(400);
        let t = truncate(&s);
        assert!(t.len() <= 512 + '…'.len_utf8());
        assert!(t.ends_with('…'));
        assert_eq!(truncate("short"), "short");
    }

    #[test]
    fn a_reqwest_url_loses_its_api_key() {
        let shown = "error sending request for url \
            (http://metadata:8090/3/tv/910001?language=en-US&api_key=0123456789abcdef)";
        assert_eq!(
            sanitize(shown),
            "error sending request for url \
            (http://metadata:8090/3/tv/910001?language=en-US&api_key=<redacted>)"
        );
    }

    #[test]
    fn every_secret_parameter_is_redacted_and_the_rest_kept() {
        assert_eq!(
            redact("GET /x?API_KEY=a&page=2&token=b#frag and ?key=c d"),
            "GET /x?API_KEY=<redacted>&page=2&token=<redacted>#frag and ?key=<redacted> d"
        );
        assert_eq!(redact("a ? b & c"), "a ? b & c");
        assert_eq!(redact("trailing ?api_key="), "trailing ?api_key=<redacted>");
    }

    #[test]
    fn url_user_information_is_redacted() {
        assert_eq!(
            redact("for url (https://user:hunter2@host/p)"),
            "for url (https://<redacted>@host/p)"
        );
        assert_eq!(redact("mail me@host"), "mail me@host");
        assert_eq!(redact("http://a/b?x@y"), "http://a/b?x@y");
    }
}
