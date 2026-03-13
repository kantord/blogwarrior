use std::time::Duration;

pub(crate) fn http_client() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent("Mozilla/5.0 (compatible; blogtato RSS reader)")
        .timeout_global(Some(Duration::from_secs(10)))
        .max_idle_connections(0)
        .build()
        .new_agent()
}
