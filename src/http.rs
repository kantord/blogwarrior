pub(crate) fn http_client() -> anyhow::Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent(format!("blogtato/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| anyhow::anyhow!("failed to build HTTP client: {}", e))
}
