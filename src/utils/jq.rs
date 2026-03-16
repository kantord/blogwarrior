use serde::Serialize;
use serde::de::DeserializeOwned;

/// Pipe `input` through `jq` with the given filter expression.
///
/// If `filter` is `None`, returns the input as-is without invoking `jq`.
/// Otherwise, serializes `input` as JSON, runs `jq -c <filter>`, and
/// deserializes the output back to the same type.
pub(crate) fn map_through_jq<T: Serialize + DeserializeOwned>(
    input: T,
    filter: Option<&str>,
) -> anyhow::Result<T> {
    let Some(filter) = filter else {
        return Ok(input);
    };

    let input_json = serde_json::to_string(&input)
        .map_err(|e| anyhow::anyhow!("failed to serialize jq input: {e}"))?;

    let mut child = std::process::Command::new("jq")
        .arg("-c")
        .arg(filter)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                anyhow::anyhow!("jq is not installed. Install it to use ingest_filter.")
            } else {
                anyhow::anyhow!("failed to run jq: {e}")
            }
        })?;

    // Write to stdin and drop it to close the pipe. Ignore broken pipe errors
    // (jq may exit early on syntax errors before reading all input).
    let write_result = std::io::Write::write_all(
        &mut child.stdin.take().expect("stdin was piped"),
        input_json.as_bytes(),
    );

    let output = child.wait_with_output()?;

    // If jq exited successfully, propagate any write error. If jq failed,
    // prefer its stderr message over a broken pipe error.
    if output.status.success() {
        write_result?;
    }

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("jq filter failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| anyhow::anyhow!("jq produced invalid UTF-8"))?;

    serde_json::from_str(&stdout).map_err(|e| {
        let preview = if stdout.len() > 200 {
            let mut boundary = 200;
            while !stdout.is_char_boundary(boundary) {
                boundary -= 1;
            }
            format!("{}...", &stdout[..boundary])
        } else {
            stdout.trim().to_string()
        };
        anyhow::anyhow!("could not parse jq output: {e}\njq produced: {preview}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Item {
        title: String,
        link: String,
    }

    fn items(data: &[(&str, &str)]) -> Vec<Item> {
        data.iter()
            .map(|(title, link)| Item {
                title: title.to_string(),
                link: link.to_string(),
            })
            .collect()
    }

    // --- None filter returns input unchanged ---
    #[test]
    fn test_none_filter_returns_input_unchanged() {
        let input = items(&[("Post A", "https://a.com"), ("Post B", "https://b.com")]);
        let expected = input.clone();
        let output: Vec<Item> = map_through_jq(input, None).unwrap();
        assert_eq!(output, expected);
    }

    // --- Identity filter ---
    #[test]
    fn test_identity_returns_input_unchanged() {
        let input = items(&[("Post A", "https://a.com"), ("Post B", "https://b.com")]);
        let expected = input.clone();
        let output: Vec<Item> = map_through_jq(input, Some(".")).unwrap();
        assert_eq!(output, expected);
    }

    // --- Filtering items ---
    #[rstest]
    #[case::some_items_match(
        &[("Video", "https://youtube.com/watch?v=1"), ("Short", "https://youtube.com/shorts/1")],
        r#"map(select(.link | test("/shorts/") | not))"#,
        &[("Video", "https://youtube.com/watch?v=1")]
    )]
    #[case::all_items_match(
        &[("Short A", "https://youtube.com/shorts/1"), ("Short B", "https://youtube.com/shorts/2")],
        r#"map(select(.link | test("/shorts/") | not))"#,
        &[]
    )]
    #[case::no_items_match(
        &[("Video A", "https://a.com"), ("Video B", "https://b.com")],
        r#"map(select(.link | test("/shorts/") | not))"#,
        &[("Video A", "https://a.com"), ("Video B", "https://b.com")]
    )]
    fn test_filtering(
        #[case] input: &[(&str, &str)],
        #[case] filter: &str,
        #[case] expected: &[(&str, &str)],
    ) {
        let output: Vec<Item> = map_through_jq(items(input), Some(filter)).unwrap();
        assert_eq!(output, items(expected));
    }

    // --- Modifying items ---
    #[test]
    fn test_map_modifies_fields() {
        let input = items(&[("Hello", "https://a.com")]);
        let output: Vec<Item> =
            map_through_jq(input, Some(r#"map(.title = "PREFIX: " + .title)"#)).unwrap();
        assert_eq!(output[0].title, "PREFIX: Hello");
        assert_eq!(output[0].link, "https://a.com");
    }

    // --- Empty input ---
    #[test]
    fn test_empty_input() {
        let input: Vec<Item> = vec![];
        let output: Vec<Item> = map_through_jq(input, Some(".")).unwrap();
        assert!(output.is_empty());
    }

    // --- Error: invalid jq expression ---
    #[test]
    fn test_invalid_filter_returns_error() {
        let input = items(&[("A", "https://a.com")]);
        let err = map_through_jq(input, Some("[invalid")).unwrap_err();
        assert!(err.to_string().contains("jq filter failed"), "got: {err}");
    }

    // --- Error: output shape mismatch ---
    #[test]
    fn test_output_type_mismatch_returns_descriptive_error() {
        // jq outputs a string, but we expect Vec<Item>
        let input = items(&[("A", "https://a.com")]);
        let err = map_through_jq(input, Some(r#"[.[].title]"#)).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("could not parse jq output"), "got: {msg}");
    }

    // --- Error: missing field in output ---
    #[test]
    fn test_missing_field_error_is_descriptive() {
        let input = items(&[("A", "https://a.com")]);
        let err = map_through_jq(input, Some(r#"map({title: .title})"#)).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("could not parse jq output"), "got: {msg}");
        assert!(
            msg.contains("link"),
            "should mention missing field, got: {msg}"
        );
    }
}
