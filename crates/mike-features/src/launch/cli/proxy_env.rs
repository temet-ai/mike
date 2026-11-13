use std::path::{Path, PathBuf};

use crate::config::LaunchConfig;
use crate::launch::ProxyEnv;

pub fn ca_cert_candidates(config: &LaunchConfig, cwd: Option<&Path>) -> Vec<PathBuf> {
    if let Some(explicit) = config.ca_cert.as_deref().filter(|p| !p.trim().is_empty()) {
        return vec![PathBuf::from(explicit)];
    }
    let mut out = Vec::with_capacity(2);
    out.push(proxy_forward::ca::default_ca_cert_path());
    out.extend(cwd.map(|cwd| cwd.join(".deploy").join("proxy-forward-ca.pem")));
    out
}

pub fn resolve_ca_cert(config: &LaunchConfig, cwd: Option<&Path>) -> Option<PathBuf> {
    ca_cert_candidates(config, cwd)
        .into_iter()
        .find(|p| std::fs::metadata(p).is_ok_and(|m| m.is_file()))
}

pub fn resolve_proxy_env(config: &LaunchConfig, cwd: Option<&Path>) -> Option<ProxyEnv> {
    let ca_cert = resolve_ca_cert(config, cwd)?;
    Some(ProxyEnv {
        https_proxy: config.proxy_forward_url.clone(),
        no_proxy: config.no_proxy.clone(),
        ca_cert,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_explicit_path_is_the_only_one() {
        let config = LaunchConfig {
            ca_cert: Some("/x/ca.pem".into()),
            ..LaunchConfig::default()
        };
        assert_eq!(
            ca_cert_candidates(&config, Some(Path::new("/repo"))),
            vec![PathBuf::from("/x/ca.pem")]
        );
    }

    #[test]
    fn candidates_default_end_with_the_deploy_copy() {
        let candidates = ca_cert_candidates(&LaunchConfig::default(), Some(Path::new("/repo")));
        assert_eq!(
            candidates.last(),
            Some(&PathBuf::from("/repo/.deploy/proxy-forward-ca.pem"))
        );
        assert_eq!(
            candidates.first(),
            Some(&proxy_forward::ca::default_ca_cert_path())
        );
    }

    #[test]
    fn resolve_proxy_env_none_without_a_cert_and_full_with_the_deploy_copy() -> anyhow::Result<()> {
        let repo = tempfile::tempdir()?;
        let config = LaunchConfig {
            ca_cert: Some(
                repo.path()
                    .join("missing.pem")
                    .to_string_lossy()
                    .into_owned(),
            ),
            ..LaunchConfig::default()
        };
        assert_eq!(resolve_proxy_env(&config, Some(repo.path())), None);

        std::fs::create_dir(repo.path().join(".deploy"))?;
        let cert = repo.path().join(".deploy").join("proxy-forward-ca.pem");
        std::fs::write(&cert, "pem")?;
        let config = LaunchConfig {
            ca_cert: None,
            ..LaunchConfig::default()
        };
        let env = resolve_proxy_env(&config, Some(repo.path()));
        let Some(env) = env else {
            anyhow::bail!("expected the deploy copy to be found unless the data-dir cert exists");
        };
        assert_eq!(env.https_proxy, "http://127.0.0.1:9987");
        assert_eq!(env.no_proxy, "localhost,127.0.0.1");
        assert!(env.ca_cert == cert || env.ca_cert == proxy_forward::ca::default_ca_cert_path());
        Ok(())
    }

    #[test]
    fn candidates_without_a_working_directory_skip_the_deploy_copy() {
        let candidates = ca_cert_candidates(&LaunchConfig::default(), None);
        assert!(
            candidates
                .iter()
                .all(|p| !p.ends_with(".deploy/proxy-forward-ca.pem"))
        );
    }
}
