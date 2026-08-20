use std::time::Duration;

use ureq::{Agent, ResponseExt};
use url::Url;

use super::fetch::{Fetch, FetchError, FetchRequest, FetchResponse};

const USER_AGENT: &str = concat!("textsurf/", env!("CARGO_PKG_VERSION"));

pub struct UreqFetch {
    agent: Agent,
}

impl UreqFetch {
    pub fn new() -> Self {
        let agent = Agent::config_builder()
            .user_agent(USER_AGENT)
            .timeout_global(Some(Duration::from_secs(30)))
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
        let final_url = Url::parse(&response.get_uri().to_string())
            .map_err(|error| FetchError::Network(format!("bad final url: {error}")))?;
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let body = response
            .into_body()
            .read_to_vec()
            .map_err(|error| FetchError::Network(format!("body read: {error}")))?;
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
    use std::io::{BufRead, Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn serve(handler: impl FnOnce(&mut std::net::TcpStream) + std::marker::Send + 'static) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback bind");
        let addr = listener.local_addr().expect("local addr");
        let handle = thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                handler(&mut stream);
                break;
            }
        });
        let fetch = UreqFetch::new();
        let request = FetchRequest {
            url: Url::parse(&format!("http://{addr}/hello")).expect("url"),
        };
        let result = fetch.fetch(&request);
        handle.join().expect("server join");
        assert!(result.is_ok(), "fetch failed: {result:?}");
        let response = result.unwrap();
        assert_eq!(&response.body, b"<p>hi</p>");
        assert_eq!(response.final_url.as_str(), &format!("http://{addr}/hello"));
    }

    #[test]
    fn fetches_over_loopback_http() {
        serve(|stream| {
            let mut reader = std::io::BufReader::new(&mut *stream);
            let mut request_line = String::new();
            reader.read_line(&mut request_line).unwrap();
            assert!(request_line.starts_with("GET /hello HTTP/1.1"));
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: 9\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<p>hi</p>").unwrap();
        });
    }

    #[test]
    fn http_errors_surface_as_status() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback bind");
        let addr = listener.local_addr().expect("local addr");
        let thread = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut drain = [0u8; 512];
            let _ = stream.read(&mut drain);
            write!(
                stream,
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
        });
        let fetch = UreqFetch::new();
        let request = FetchRequest {
            url: Url::parse(&format!("http://{addr}/missing")).expect("url"),
        };
        let result = fetch.fetch(&request);
        thread.join().expect("server join");
        assert_eq!(result, Err(FetchError::HttpStatus(404)));
    }
}
