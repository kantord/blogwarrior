pub(crate) fn http_client() -> anyhow::Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (compatible; blogtato RSS reader)")
        .timeout(std::time::Duration::from_secs(10))
        .pool_max_idle_per_host(0)
        .build()
        .map_err(|e| anyhow::anyhow!("failed to build HTTP client: {}", e))
}
