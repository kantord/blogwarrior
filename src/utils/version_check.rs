use std::time::Duration;

pub(crate) struct VersionStatus {
    pub current: String,
    pub latest: String,
}

const VERSION_CHECK_TIMEOUT: Duration = Duration::from_secs(3);

pub(crate) fn check_for_newer_version(
    crates_io_url: &str,
    current_version: &str,
) -> anyhow::Result<Option<VersionStatus>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(VERSION_CHECK_TIMEOUT)
        .build()?;
    let body = client.get(crates_io_url).send()?.text()?;
    let response: serde_json::Value = serde_json::from_str(&body)?;

    let latest = response["crate"]["max_version"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing max_version in response"))?;

    if latest != current_version {
        Ok(Some(VersionStatus {
            current: current_version.to_string(),
            latest: latest.to_string(),
        }))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use rstest::rstest;

    fn mock_crates_io(server: &MockServer, version: &str) {
        server.mock(|when, then| {
            when.method(GET).path("/api/v1/crates/blogtato");
            then.status(200)
                .header("Content-Type", "application/json")
                .body(
                    serde_json::json!({
                        "crate": {
                            "max_version": version
                        }
                    })
                    .to_string(),
                );
        });
    }

    #[rstest]
    #[case::newer_version_available("0.1.14", "0.2.0", true, "0.2.0")]
    #[case::same_version("0.1.14", "0.1.14", false, "")]
    #[case::patch_update("0.1.14", "0.1.15", true, "0.1.15")]
    #[case::major_update("0.1.14", "1.0.0", true, "1.0.0")]
    #[case::remote_is_older("0.2.0", "0.1.14", true, "0.1.14")]
    fn test_version_check(
        #[case] current: &str,
        #[case] remote: &str,
        #[case] expect_warning: bool,
        #[case] expected_latest: &str,
    ) {
        let server = MockServer::start();
        mock_crates_io(&server, remote);

        let url = format!("{}/api/v1/crates/blogtato", server.base_url());
        let result = check_for_newer_version(&url, current).unwrap();

        if expect_warning {
            let status = result.expect("expected a version warning");
            assert_eq!(status.current, current);
            assert_eq!(status.latest, expected_latest);
        } else {
            assert!(result.is_none(), "expected no warning for same version");
        }
    }

    #[test]
    fn test_version_check_handles_network_error() {
        let result = check_for_newer_version("http://localhost:1", "0.1.14");
        assert!(result.is_err());
    }
}
