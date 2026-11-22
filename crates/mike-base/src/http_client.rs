pub const PRODUCT: &str = "mike";

pub fn user_agent(component: &str) -> String {
    format!("{PRODUCT}/{} {component}", env!("CARGO_PKG_VERSION"))
}

pub fn builder(component: &str) -> reqwest::ClientBuilder {
    reqwest::Client::builder().user_agent(user_agent(component))
}

pub fn client(component: &str) -> reqwest::Client {
    builder(component).build().unwrap_or_else(|error| {
        tracing::warn!(%error, component, "http client: building failed, using one without a user agent");
        reqwest::Client::new()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agent_names_mike_its_version_and_the_component() {
        let agent = user_agent("fleet-cli");
        assert!(agent.starts_with("mike/"), "{agent}");
        assert!(agent.ends_with(" fleet-cli"), "{agent}");
    }
}
