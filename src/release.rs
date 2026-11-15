/// Reads `tag_name` from the body of a GitHub "latest release" response.
pub fn latest_tag(body: &str) -> Option<&str> {
    let after_key = &body[body.find("\"tag_name\"")? + "\"tag_name\"".len()..];
    let value = after_key.trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    Some(&value[..value.find('"')?])
}

fn parse(version: &str) -> Option<(u64, u64, u64)> {
    let core = version.trim_start_matches('v').split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>());
    let triple = (parts.next()?.ok()?, parts.next()?.ok()?, parts.next()?.ok()?);
    parts.next().is_none().then_some(triple)
}

pub fn is_newer(tag: &str, current: &str) -> bool {
    matches!((parse(tag), parse(current)), (Some(a), Some(b)) if a > b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_tag_from_response() {
        let body = r#"{"url":"x","tag_name": "v0.2.0","name":"v0.2.0"}"#;
        assert_eq!(latest_tag(body), Some("v0.2.0"));
        assert_eq!(latest_tag("{}"), None);
    }

    #[test]
    fn compares_versions() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("v0.10.0", "0.9.5"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("v0.0.9", "0.1.0"));
        assert!(!is_newer("nightly", "0.1.0"));
    }
}
