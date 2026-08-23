use url::Url;

pub(super) fn normalized_url(url: &Url) -> String {
    let mut normalized = url.clone();
    normalized.set_fragment(None);
    normalized.into()
}
