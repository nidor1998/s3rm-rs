use url::Url;

const INVALID_SCHEME: &str = "URL scheme must be https:// or http://";

pub fn check_scheme(url: &str) -> Result<String, String> {
    let parsed = Url::parse(url).map_err(|e| e.to_string())?;

    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err(INVALID_SCHEME.to_string());
    }

    Ok(url.to_string())
}
