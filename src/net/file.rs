use std::fs::File;
use std::io::{Read, Take};

use super::fetch::{Fetch, FetchError, FetchRequest, FetchResponse, MAX_BODY_BYTES};

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
        let file = File::open(&path)
            .map_err(|error| FetchError::Network(format!("{}: {error}", path.display())))?;
        let mut body = Vec::new();
        let mut limited: Take<File> = file.take((MAX_BODY_BYTES + 1) as u64);
        limited
            .read_to_end(&mut body)
            .map_err(|error| FetchError::Network(format!("{}: {error}", path.display())))?;
        if body.len() > MAX_BODY_BYTES {
            return Err(FetchError::BodyTooLarge {
                limit: MAX_BODY_BYTES,
            });
        }
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
    use std::io::Write;
    use tempfile::NamedTempFile;
    use url::Url;

    #[test]
    fn reads_a_local_file() {
        let mut file = NamedTempFile::new().expect("temp file");
        file.write_all(b"<h1>local</h1>").expect("write temp file");
        let url = Url::from_file_path(file.path()).expect("file url");
        let response = FileFetch.fetch(&FetchRequest { url }).expect("fetch");
        assert_eq!(response.body, b"<h1>local</h1>");
    }

    #[test]
    fn rejects_files_larger_than_the_body_limit() {
        let file = NamedTempFile::new().expect("temp file");
        file.as_file()
            .set_len((MAX_BODY_BYTES + 1) as u64)
            .expect("size temp file");
        let url = Url::from_file_path(file.path()).expect("file url");
        assert_eq!(
            FileFetch.fetch(&FetchRequest { url }),
            Err(FetchError::BodyTooLarge {
                limit: MAX_BODY_BYTES
            })
        );
    }

    #[test]
    fn missing_file_is_a_network_error() {
        let url = Url::from_file_path(
            std::env::temp_dir().join("textsurfer-file-test-definitely-absent"),
        )
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
