use std::time::Duration;

/// Note: ureq enforces a default 10 MB limit on `read_to_vec()`/`read_to_string()`
/// and a default cap of 10 redirects (with error on exceed),
/// so all call sites are protected without explicit caps.
pub(crate) fn http_client() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent("Mozilla/5.0 (compatible; blogtato RSS reader)")
        .timeout_global(Some(Duration::from_secs(10)))
        .max_idle_connections(0)
        .build()
        .new_agent()
}
