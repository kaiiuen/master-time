//! Server profiles and the built-in NTP server catalog.
//!
//! This module contains catalog data and validation only. It does not perform
//! DNS lookups, network requests, or store credentials.

use std::{fmt, net::IpAddr, str::FromStr};

/// A named NTP endpoint with an optional address to use if hostname resolution
/// is unavailable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerProfile {
    name: String,
    hostname: String,
    ip_fallback: Option<IpAddr>,
}

impl ServerProfile {
    /// Creates a profile after validating its name, hostname, and fallback.
    pub fn new(
        name: impl Into<String>,
        hostname: impl Into<String>,
        ip_fallback: Option<IpAddr>,
    ) -> Result<Self, ProfileError> {
        let name = name.into();
        let hostname = hostname.into();

        if name.trim().is_empty() {
            return Err(ProfileError::EmptyName);
        }
        if name.chars().any(char::is_control) {
            return Err(ProfileError::InvalidName);
        }
        validate_hostname(&hostname)?;

        Ok(Self {
            name,
            hostname,
            ip_fallback,
        })
    }

    /// Convenience constructor for input commonly received as text.
    pub fn from_strings(
        name: impl Into<String>,
        hostname: impl Into<String>,
        ip_fallback: Option<&str>,
    ) -> Result<Self, ProfileError> {
        let fallback = ip_fallback
            .filter(|value| !value.trim().is_empty())
            .map(|value| IpAddr::from_str(value.trim()))
            .transpose()
            .map_err(|_| ProfileError::InvalidIpFallback)?;
        Self::new(name, hostname, fallback)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn hostname(&self) -> &str {
        &self.hostname
    }

    pub fn ip_fallback(&self) -> Option<IpAddr> {
        self.ip_fallback
    }
}

/// Errors returned when constructing a server profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileError {
    EmptyName,
    InvalidName,
    EmptyHostname,
    HostnameTooLong,
    InvalidHostname,
    InvalidHostnameLabel,
    InvalidIpFallback,
}

impl fmt::Display for ProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::EmptyName => "server name cannot be empty",
            Self::InvalidName => "server name contains a control character",
            Self::EmptyHostname => "hostname cannot be empty",
            Self::HostnameTooLong => "hostname is too long",
            Self::InvalidHostname => "hostname contains invalid characters",
            Self::InvalidHostnameLabel => "hostname label is invalid",
            Self::InvalidIpFallback => "IP fallback is invalid",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ProfileError {}

fn validate_hostname(hostname: &str) -> Result<(), ProfileError> {
    if hostname.is_empty() {
        return Err(ProfileError::EmptyHostname);
    }
    if hostname.len() > 253 || !hostname.is_ascii() {
        return Err(ProfileError::HostnameTooLong);
    }
    if hostname.starts_with('.') || hostname.ends_with('.') {
        return Err(ProfileError::InvalidHostname);
    }

    for label in hostname.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err(ProfileError::InvalidHostnameLabel);
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(ProfileError::InvalidHostnameLabel);
        }
        if !label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(ProfileError::InvalidHostname);
        }
    }
    Ok(())
}

/// The broad group used to filter catalog entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Corporate,
    Distribution,
    Standards,
    Pool,
    Vendor,
}

impl Category {
    pub const ALL: [Self; 5] = [
        Self::Corporate,
        Self::Distribution,
        Self::Standards,
        Self::Pool,
        Self::Vendor,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Corporate => "Corporate",
            Self::Distribution => "OS & Distribution",
            Self::Standards => "Standards",
            Self::Pool => "NTP Pools",
            Self::Vendor => "Vendor",
        }
    }
}

/// One profile and the catalog information shown alongside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    profile: ServerProfile,
    category: Category,
    strategy: &'static str,
    notes: &'static str,
}

impl CatalogEntry {
    pub fn profile(&self) -> &ServerProfile {
        &self.profile
    }

    pub const fn category(&self) -> Category {
        self.category
    }

    pub const fn strategy(&self) -> &'static str {
        self.strategy
    }

    pub const fn notes(&self) -> &'static str {
        self.notes
    }
}

/// An owned catalog that can be extended with application-specific entries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerCatalog {
    entries: Vec<CatalogEntry>,
}

impl ServerCatalog {
    /// Returns the initial built-in catalog.
    pub fn built_in() -> Self {
        Self {
            entries: vec![
                entry(
                    "Google Public NTP",
                    "time.google.com",
                    Some("216.239.35.0"),
                    Category::Corporate,
                    "Anycast",
                    "Public service.",
                ),
                entry(
                    "Cloudflare Time",
                    "time.cloudflare.com",
                    Some("162.159.200.1"),
                    Category::Corporate,
                    "Anycast",
                    "Public service.",
                ),
                entry(
                    "Ubuntu",
                    "ntp.ubuntu.com",
                    Some("91.189.91.157"),
                    Category::Distribution,
                    "Canonical",
                    "Distribution default.",
                ),
                entry(
                    "NIST",
                    "time.nist.gov",
                    Some("132.163.96.1"),
                    Category::Standards,
                    "Atomic clock",
                    "US national time service.",
                ),
                entry(
                    "Global Pool",
                    "pool.ntp.org",
                    None,
                    Category::Pool,
                    "GeoDNS",
                    "Community pool.",
                ),
                entry(
                    "NZ Pool",
                    "nz.pool.ntp.org",
                    None,
                    Category::Pool,
                    "GeoDNS",
                    "New Zealand regional pool.",
                ),
            ],
        }
    }

    pub fn entries(&self) -> &[CatalogEntry] {
        &self.entries
    }

    pub fn add(&mut self, entry: CatalogEntry) {
        self.entries.push(entry);
    }

    /// Filters by category and performs a case-insensitive substring search.
    /// An empty query matches every entry in the selected category.
    pub fn filter(&self, category: Option<Category>, query: &str) -> Vec<&CatalogEntry> {
        let query = query.to_lowercase();
        self.entries
            .iter()
            .filter(|entry| category.is_none_or(|wanted| entry.category == wanted))
            .filter(|entry| {
                query.is_empty()
                    || entry.profile.name.to_lowercase().contains(&query)
                    || entry.profile.hostname.to_lowercase().contains(&query)
                    || entry.category.as_str().to_lowercase().contains(&query)
            })
            .collect()
    }
}

fn entry(
    name: &'static str,
    hostname: &'static str,
    fallback: Option<&'static str>,
    category: Category,
    strategy: &'static str,
    notes: &'static str,
) -> CatalogEntry {
    CatalogEntry {
        profile: ServerProfile::from_strings(name, hostname, fallback)
            .expect("built-in server catalog contains valid profiles"),
        category,
        strategy,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_hostname_and_optional_fallback() {
        let profile =
            ServerProfile::from_strings("Local", "Time.Example.test", Some("192.0.2.1")).unwrap();
        assert_eq!(profile.hostname(), "Time.Example.test");
        assert_eq!(profile.ip_fallback(), Some(IpAddr::from([192, 0, 2, 1])));
        assert!(ServerProfile::from_strings("bad", "-example.test", None).is_err());
        assert!(ServerProfile::from_strings("bad", "example.test", Some("not-an-ip")).is_err());
    }

    #[test]
    fn built_in_catalog_is_small_and_valid() {
        let catalog = ServerCatalog::built_in();
        assert_eq!(catalog.entries().len(), 6);
        assert!(
            catalog
                .entries()
                .iter()
                .all(|entry| !entry.profile().hostname().is_empty())
        );
    }

    #[test]
    fn filters_category_and_search_case_insensitively() {
        let catalog = ServerCatalog::built_in();
        let standards = catalog.filter(Some(Category::Standards), "NIST");
        assert_eq!(standards.len(), 1);
        assert_eq!(standards[0].profile().hostname(), "time.nist.gov");
        assert_eq!(catalog.filter(None, "CLOUDFLARE").len(), 1);
        assert_eq!(catalog.filter(Some(Category::Pool), "").len(), 2);
    }
}
