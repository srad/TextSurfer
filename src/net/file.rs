use std::fs;

use super::fetch::{Fetch, FetchError, FetchRequest, FetchResponse};

pub struct FileFetch;

impl Fetch for FileFetch {
    fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        if request.url.scheme() != "file" {
            return Err(FetchError::UnsupportedScheme(
                request.url.scheme().to_string(),
            ));
        }
        let path = request
            .url
            .to_file_path()
            .map_err(|_| FetchError::Network(format!("{} has no local path", request.url)))?;
        let body = fs::read(&path)
            .map_err(|error| FetchError::Network(format!("{}: {error}", path.display())))?;
        Ok(FetchResponse {
            final_url: request.url.clone(),
            body,
            content_type: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use url::Url;

    #[test]
    fn reads_a_local_file() {
        let path = std::env::temp_dir().join(format!("textsurf-file-test-{}", std::process::id()));
        fs::write(&path, b"<h1>local</h1>").expect("write temp file");
        let url = Url::from_file_path(&path).expect("file url");
        let response = FileFetch.fetch(&FetchRequest { url }).expect("fetch");
        assert_eq!(response.body, b"<h1>local</h1>");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn missing_file_is_a_network_error() {
        let url =
            Url::from_file_path(std::env::temp_dir().join("textsurf-file-test-definitely-absent"))
                .expect("file url");
        let result = FileFetch.fetch(&FetchRequest { url });
        assert!(matches!(result, Err(FetchError::Network(_))));
    }

    #[test]
    fn non_file_scheme_is_rejected() {
        let url = Url::parse("http://example.com/").expect("url");
        let result = FileFetch.fetch(&FetchRequest { url });
        assert_eq!(
            result,
            Err(FetchError::UnsupportedScheme("http".to_string()))
        );
    }
}
