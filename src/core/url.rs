use url::Url;

const SEARCH_ENDPOINT: &str = "https://lite.duckduckgo.com/lite/";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheme {
    Http,
    Https,
    File,
    About,
    Unknown,
}

pub fn parse_scheme(input: &str) -> Scheme {
    let lower = input.to_ascii_lowercase();
    if lower.starts_with("http://") {
        Scheme::Http
    } else if lower.starts_with("https://") {
        Scheme::Https
    } else if lower.starts_with("file://") {
        Scheme::File
    } else if lower.starts_with("about:") {
        Scheme::About
    } else {
        Scheme::Unknown
    }
}

fn looks_like_host(input: &str) -> bool {
    let (host, port) = match input.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (input, None),
    };
    let host_ok = host == "localhost"
        || (host.contains('.') && !host.starts_with('.') && !host.ends_with('.'));
    if !host_ok {
        return false;
    }
    match port {
        None => true,
        Some(port) => !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()),
    }
}

fn search_url(terms: &str) -> String {
    let mut url = Url::parse(SEARCH_ENDPOINT).expect("static search endpoint must parse");
    url.query_pairs_mut().append_pair("q", terms);
    url.to_string()
}

pub fn url_fix(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if parse_scheme(trimmed) != Scheme::Unknown {
        return trimmed.to_string();
    }
    if looks_like_host(trimmed) {
        return format!("https://{trimmed}");
    }
    search_url(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn arbitrary_text() -> impl Strategy<Value = String> {
        "\\PC{0,64}"
    }

    proptest! {
        #[test]
        fn url_fix_is_idempotent(s in arbitrary_text()) {
            let once = url_fix(&s);
            prop_assert_eq!(url_fix(&once), once);
        }

        #[test]
        fn non_empty_input_produces_non_empty_output(s in "\\PC{1,64}") {
            prop_assume!(!s.trim().is_empty());
            prop_assert!(!url_fix(&s).is_empty());
        }

        #[test]
        fn terms_without_a_scheme_route_to_the_search_engine(s in "\\PC{1,64}") {
            prop_assume!(parse_scheme(&s) == Scheme::Unknown);
            prop_assume!(!looks_like_host(&s));
            prop_assume!(!s.trim().is_empty());
            let out = url_fix(&s);
            prop_assert!(out.starts_with(SEARCH_ENDPOINT));
        }
    }

    #[test]
    fn recognized_schemes_pass_through_unmodified() {
        for prefix in ["http://", "https://", "file://", "about:"] {
            let input = format!("{prefix}example.com/a b");
            assert_eq!(url_fix(&input), input);
        }
    }

    #[test]
    fn host_like_terms_get_an_https_prefix() {
        assert_eq!(url_fix("example.com"), "https://example.com");
        assert_eq!(url_fix("localhost:8080"), "https://localhost:8080");
    }

    #[test]
    fn bare_terms_route_to_the_search_engine() {
        assert_eq!(
            url_fix("rust text browser"),
            format!("{SEARCH_ENDPOINT}?q=rust+text+browser")
        );
    }

    #[test]
    fn whitespace_is_trimmed_first() {
        assert_eq!(url_fix("  https://example.com  "), "https://example.com");
        assert_eq!(url_fix("   "), "");
    }

    #[test]
    fn scheme_case_is_insensitive() {
        assert_eq!(parse_scheme("HTTPS://example.com"), Scheme::Https);
        assert_eq!(url_fix("About:blank"), "About:blank");
    }
}
