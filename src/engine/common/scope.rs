//! Port of the reference crawler `pkg/utils/scope` — scope manager with DNS-based field scope
//! (dn/rdn/fqdn/custom regex) and URL regex in/out-of-scope rules.

use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DnsScopeField {
    Dn,
    Rdn,
    Fqdn,
    Custom,
}

/// Manages scope validation for the crawl (reference crawler `scope.Manager`).
#[derive(Debug, Clone)]
pub struct ScopeManager {
    in_scope: Vec<Regex>,
    out_of_scope: Vec<Regex>,
    no_scope: bool,
    field_scope: DnsScopeField,
    field_scope_pattern: Option<Regex>,
}

impl ScopeManager {
    /// Create a scope manager (reference crawler `NewManager`).
    pub fn new(
        in_scope: &[String],
        out_of_scope: &[String],
        field_scope: &str,
        no_scope: bool,
    ) -> Result<Self, String> {
        let mut manager = ScopeManager {
            in_scope: Vec::new(),
            out_of_scope: Vec::new(),
            no_scope,
            field_scope: DnsScopeField::Rdn,
            field_scope_pattern: None,
        };

        match field_scope {
            "dn" => manager.field_scope = DnsScopeField::Dn,
            "rdn" => manager.field_scope = DnsScopeField::Rdn,
            "fqdn" => manager.field_scope = DnsScopeField::Fqdn,
            custom => {
                manager.field_scope = DnsScopeField::Custom;
                manager.field_scope_pattern = Some(
                    Regex::new(custom)
                        .map_err(|e| format!("could not compile regex {custom}: {e}"))?,
                );
            }
        }

        for r in in_scope {
            manager.in_scope.push(
                Regex::new(r).map_err(|e| format!("could not compile regex {r}: {e}"))?,
            );
        }
        for r in out_of_scope {
            manager.out_of_scope.push(
                Regex::new(r).map_err(|e| format!("could not compile regex {r}: {e}"))?,
            );
        }
        Ok(manager)
    }

    /// Validate a URL against scope rules (reference crawler `Manager.Validate`).
    /// When `no_scope` is true DNS validation is skipped but URL regex rules
    /// still apply.
    pub fn validate(&self, url: &url::Url, root_hostname: &str) -> bool {
        if !self.no_scope {
            let hostname = url.host_str().unwrap_or("");
            if !self.validate_dns(hostname, root_hostname) {
                return false;
            }
        }
        if !self.in_scope.is_empty() || !self.out_of_scope.is_empty() {
            return self.validate_url(&url.to_string());
        }
        true
    }

    /// URL regex scope: out-of-scope excludes, in-scope must match when present
    /// (reference crawler `validateURL`).
    pub fn validate_url(&self, url: &str) -> bool {
        for item in &self.out_of_scope {
            if item.is_match(url) {
                return false;
            }
        }
        if self.in_scope.is_empty() {
            return true;
        }
        self.in_scope.iter().any(|item| item.is_match(url))
    }

    /// DNS-based scope validation (reference crawler `validateDNS`).
    fn validate_dns(&self, hostname: &str, root_hostname: &str) -> bool {
        let is_ip = hostname.parse::<std::net::IpAddr>().is_ok();

        if self.field_scope == DnsScopeField::Custom {
            if let Some(pattern) = &self.field_scope_pattern {
                if pattern.is_match(hostname) {
                    return true;
                }
            }
        }
        if self.field_scope == DnsScopeField::Fqdn || is_ip {
            return hostname.eq_ignore_ascii_case(root_hostname);
        }

        let rdn = crate::output::fields::etld_plus_one(root_hostname);
        match self.field_scope {
            // dn: any host containing the domain-name keyword — the first
            // label of the registrable domain ("example" for sub.example.com),
            // NOT the full eTLD+1 (reference crawler getDomainRDNandRDN).
            DnsScopeField::Dn => {
                let dn = rdn.split('.').next().unwrap_or(&rdn).to_lowercase();
                hostname.to_lowercase().contains(&dn)
            }
            // rdn: the registrable domain itself or any of its subdomains,
            // requiring a label boundary (evilexample.com != example.com).
            DnsScopeField::Rdn => matches_domain_or_subdomain(hostname, &rdn),
            _ => false,
        }
    }
}

/// Case-insensitive equality or subdomain check with label boundary
/// (reference crawler `matchesDomainOrSubdomain`).
pub fn matches_domain_or_subdomain(host: &str, domain: &str) -> bool {
    let host = host.to_lowercase();
    let domain = domain.to_lowercase();
    host == domain || host.ends_with(&format!(".{domain}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(u: &str) -> url::Url {
        url::Url::parse(u).unwrap()
    }

    #[test]
    fn test_rdn_scope() {
        let m = ScopeManager::new(&[], &[], "rdn", false).unwrap();
        assert!(m.validate(&url("https://app.example.com/x"), "example.com"));
        assert!(m.validate(&url("https://example.com"), "example.com"));
        assert!(!m.validate(&url("https://evilexample.com"), "example.com"));
        assert!(!m.validate(&url("https://other.org"), "example.com"));
    }

    #[test]
    fn test_fqdn_scope() {
        let m = ScopeManager::new(&[], &[], "fqdn", false).unwrap();
        assert!(m.validate(&url("https://example.com"), "example.com"));
        assert!(!m.validate(&url("https://sub.example.com"), "example.com"));
    }

    #[test]
    fn test_dn_scope() {
        let m = ScopeManager::new(&[], &[], "dn", false).unwrap();
        assert!(m.validate(&url("https://www.example.com"), "example.com"));
        // dn is a keyword match: evilexample.com contains "example.com"
        assert!(m.validate(&url("https://evilexample.com"), "example.com"));
    }

    #[test]
    fn test_custom_regex_scope() {
        let m = ScopeManager::new(&[], &[], "(company-staging\\.io|company\\.com)", false).unwrap();
        assert!(m.validate(&url("https://app.company.com"), "company.com"));
        assert!(m.validate(&url("https://company-staging.io"), "company.com"));
        assert!(!m.validate(&url("https://other.com"), "company.com"));
    }

    #[test]
    fn test_in_out_scope_regex() {
        let m = ScopeManager::new(
            &["/api/".to_string()],
            &["logout".to_string()],
            "rdn",
            false,
        )
        .unwrap();
        assert!(m.validate(&url("https://example.com/api/v1"), "example.com"));
        assert!(!m.validate(&url("https://example.com/api/logout"), "example.com"));
        assert!(!m.validate(&url("https://example.com/static"), "example.com"));
    }

    #[test]
    fn test_no_scope_skips_dns() {
        let m = ScopeManager::new(&[], &[], "rdn", true).unwrap();
        assert!(m.validate(&url("https://external.org/x"), "example.com"));
    }

    #[test]
    fn test_ip_fallback_exact_match() {
        let m = ScopeManager::new(&[], &[], "rdn", false).unwrap();
        assert!(m.validate(&url("http://127.0.0.1:8080/x"), "127.0.0.1"));
        assert!(!m.validate(&url("http://127.0.0.2/x"), "127.0.0.1"));
    }

    #[test]
    fn test_label_boundary() {
        assert!(matches_domain_or_subdomain("a.example.com", "example.com"));
        assert!(!matches_domain_or_subdomain("evilexample.com", "example.com"));
        assert!(matches_domain_or_subdomain("EXAMPLE.com", "example.COM"));
    }
}
