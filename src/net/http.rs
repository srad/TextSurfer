use std::time::Duration;

use ureq::{Agent, ResponseExt};
use url::Url;

use super::fetch::{Fetch, FetchError, FetchRequest, FetchResponse, MAX_BODY_BYTES};

const USER_AGENT: &str = concat!("textsurfer/", env!("CARGO_PKG_VERSION"));

pub struct UreqFetch {
    agent: Agent,
}

impl UreqFetch {
    pub fn new() -> Self {
        Self::with_user_agent(USER_AGENT)
    }

    pub fn with_user_agent(user_agent: impl Into<String>) -> Self {
        let agent = Agent::config_builder()
            .user_agent(user_agent.into())
            .timeout_global(Some(Duration::from_secs(30)))
            .save_redirect_history(true)
            .build()
            .new_agent();
        Self { agent }
    }
}

impl Default for UreqFetch {
    fn default() -> Self {
        Self::new()
    }
}

impl Fetch for UreqFetch {
    fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        let response = match self.agent.get(request.url.as_str()).call() {
            Ok(response) => response,
            Err(ureq::Error::StatusCode(status)) => {
                return Err(FetchError::HttpStatus(status));
            }
            Err(error) => return Err(FetchError::Network(error.to_string())),
        };
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(FetchError::HttpStatus(status));
        }
        let final_url = response
            .get_redirect_history()
            .and_then(|history| history.last())
            .map(|uri| Url::parse(&uri.to_string()))
            .transpose()
            .map_err(|error| FetchError::Network(format!("bad final url: {error}")))?
            .unwrap_or_else(|| request.url.clone());
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let body = response
            .into_body()
            .into_with_config()
            .limit(MAX_BODY_BYTES as u64)
            .read_to_vec()
            .map_err(|error| match error {
                ureq::Error::BodyExceedsLimit(_) => FetchError::BodyTooLarge {
                    limit: MAX_BODY_BYTES,
                },
                error => FetchError::Network(format!("body read: {error}")),
            })?;
        Ok(FetchResponse {
            final_url,
            body,
            content_type,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ureq::Body;

    struct SyntheticMiddleware {
        status: u16,
        body: Vec<u8>,
    }

    impl ureq::middleware::Middleware for SyntheticMiddleware {
        fn handle(
            &self,
            _request: ureq::http::Request<ureq::SendBody>,
            _next: ureq::middleware::MiddlewareNext,
        ) -> Result<ureq::http::Response<Body>, ureq::Error> {
            Ok(ureq::http::Response::builder()
                .status(self.status)
                .header("content-type", "text/html; charset=utf-8")
                .body(Body::builder().data(self.body.clone()))
                .expect("synthetic response"))
        }
    }

    fn synthetic_fetch(status: u16, body: Vec<u8>) -> UreqFetch {
        let agent = Agent::config_builder()
            .user_agent("synthetic-agent")
            .middleware(SyntheticMiddleware { status, body })
            .build()
            .new_agent();
        UreqFetch { agent }
    }

    #[test]
    fn fetches_through_synthetic_middleware_with_configured_headers() {
        let fetch = synthetic_fetch(200, b"<p>hi</p>".to_vec());
        let request = FetchRequest {
            url: Url::parse("https://example.com/hello").unwrap(),
        };
        let response = fetch.fetch(&request).unwrap();
        assert_eq!(response.body, b"<p>hi</p>");
        assert_eq!(response.final_url, request.url);
        assert!(matches!(
            fetch.agent.config().user_agent(),
            ureq::config::AutoHeaderValue::Provided(value) if value.as_str() == "synthetic-agent"
        ));
    }

    #[test]
    fn http_errors_surface_as_status() {
        let fetch = synthetic_fetch(404, Vec::new());
        let request = FetchRequest {
            url: Url::parse("https://example.com/missing").expect("url"),
        };
        let result = fetch.fetch(&request);
        assert_eq!(result, Err(FetchError::HttpStatus(404)));
    }

    #[test]
    fn oversized_synthetic_bodies_are_rejected() {
        let fetch = synthetic_fetch(200, vec![0; MAX_BODY_BYTES + 1]);
        let request = FetchRequest {
            url: Url::parse("https://example.com/large").expect("url"),
        };
        assert_eq!(
            fetch.fetch(&request),
            Err(FetchError::BodyTooLarge {
                limit: MAX_BODY_BYTES
            })
        );
    }
}
