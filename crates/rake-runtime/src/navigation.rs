use std::process::Command;

use rake_config::RakeConfig;
use url::{Host, Url};

pub struct NavigationPolicy {
    hosts: Vec<Host<String>>,
}

impl NavigationPolicy {
    pub fn new(config: &RakeConfig) -> Self {
        let start = Url::parse(&config.url).expect("config URL was validated");
        let mut hosts = vec![start.host().expect("config URL has a host").to_owned()];
        hosts.extend(
            config
                .safe_domains
                .iter()
                .map(|domain| Host::parse(domain).expect("safe domain was validated")),
        );
        Self { hosts }
    }

    fn allows(&self, url: &str) -> bool {
        let Ok(url) = Url::parse(url) else {
            return false;
        };
        if !matches!(url.scheme(), "http" | "https") {
            return true;
        }
        let Some(host) = url.host() else {
            return false;
        };
        self.hosts.iter().any(|safe| host_matches(&host, safe))
    }

    pub fn navigate(&self, url: &str) -> bool {
        if self.allows(url) {
            return true;
        }
        if Url::parse(url).is_ok_and(|url| matches!(url.scheme(), "http" | "https")) {
            match Command::new("open").arg(url).status() {
                Ok(status) if status.success() => {}
                Ok(status) => eprintln!("Cannot open {url} in the default browser: {status}"),
                Err(error) => eprintln!("Cannot open {url} in the default browser: {error}"),
            }
        }
        false
    }
}

fn host_matches<S: AsRef<str>, T: AsRef<str>>(host: &Host<S>, safe: &Host<T>) -> bool {
    match (host, safe) {
        (Host::Domain(host), Host::Domain(safe)) => {
            let host = host.as_ref().trim_end_matches('.');
            let safe = safe.as_ref().trim_end_matches('.');
            host == safe
                || host
                    .strip_suffix(safe)
                    .is_some_and(|prefix| prefix.ends_with('.'))
        }
        (Host::Ipv4(host), Host::Ipv4(safe)) => host == safe,
        (Host::Ipv6(host), Host::Ipv6(safe)) => host == safe,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_only_exact_hosts_and_dot_delimited_subdomains() {
        let safe = Host::parse("EXAMPLE.com.").unwrap();
        for (host, expected) in [
            ("example.com", true),
            ("example.com.", true),
            ("login.example.com", true),
            ("deep.login.example.com", true),
            ("notexample.com", false),
            ("example.com.attacker.test", false),
            ("com", false),
        ] {
            let host = Host::parse(host).unwrap();
            assert_eq!(host_matches(&host, &safe), expected, "{host}");
        }
    }

    #[test]
    fn matches_ip_addresses_exactly() {
        for host in ["127.0.0.1", "[::1]"] {
            let safe = Host::parse(host).unwrap();
            assert!(host_matches(&safe, &safe));
            assert!(!host_matches(&Host::parse("example.com").unwrap(), &safe));
        }
    }

    #[test]
    fn policy_includes_start_host_and_configured_domains() {
        let mut config = RakeConfig::new("https://app.example.com/start");
        config.safe_domains = vec!["SAFE.test".into(), "bücher.example".into()];
        let policy = NavigationPolicy::new(&config);
        for url in [
            "https://app.example.com/next",
            "http://sub.app.example.com:8080/next",
            "https://safe.test",
            "https://login.safe.test",
            "https://xn--bcher-kva.example",
            "about:blank",
        ] {
            assert!(policy.allows(url), "{url}");
        }
        for url in [
            "https://example.com",
            "https://unsafe.test",
            "https://safe.test.attacker.test",
            "https://safe.test@attacker.test",
            "https://attacker.test/safe.test",
            "invalid",
        ] {
            assert!(!policy.allows(url), "{url}");
        }
    }
}
