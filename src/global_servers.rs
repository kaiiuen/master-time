//! UI-independent catalog of useful public NTP servers.
//!
//! The catalog contains hostnames rather than requiring a current IP address.
//! DNS-backed services and pool names can change their addresses over time, so
//! an IP fallback is optional and is deliberately left unset for these entries.

use crate::servers::{Category, ServerProfile};

/// A server profile together with the information useful to a browser or
/// selector UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalServerEntry {
    profile: ServerProfile,
    category: Category,
    strategy: &'static str,
    notes: &'static str,
}

impl GlobalServerEntry {
    fn new(
        name: &'static str,
        hostname: &'static str,
        category: Category,
        strategy: &'static str,
        notes: &'static str,
    ) -> Self {
        Self {
            profile: ServerProfile::from_strings(name, hostname, None)
                .expect("built-in global server catalog contains valid profiles"),
            category,
            strategy,
            notes,
        }
    }

    /// The reusable server profile used by the time service.
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

/// An owned, UI-independent catalog of global NTP server entries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlobalServerCatalog {
    entries: Vec<GlobalServerEntry>,
}

impl GlobalServerCatalog {
    /// Creates the curated built-in catalog.
    pub fn built_in() -> Self {
        Self {
            entries: vec![
                // Public services.
                entry(
                    "Google Public NTP",
                    "time.google.com",
                    Category::Corporate,
                    "Anycast",
                    "Public service.",
                ),
                entry(
                    "Cloudflare Time",
                    "time.cloudflare.com",
                    Category::Corporate,
                    "Anycast",
                    "Public service.",
                ),
                entry(
                    "Meta NTP",
                    "time.facebook.com",
                    Category::Corporate,
                    "Anycast",
                    "Public service.",
                ),
                entry(
                    "Microsoft Azure NTP",
                    "time.windows.com",
                    Category::Corporate,
                    "Anycast",
                    "Public service.",
                ),
                // Standards organizations and national services.
                entry(
                    "NIST",
                    "time.nist.gov",
                    Category::Standards,
                    "Reference clocks",
                    "US national time service.",
                ),
                entry(
                    "USNO",
                    "tick.usno.navy.mil",
                    Category::Standards,
                    "Reference clocks",
                    "US Naval Observatory service.",
                ),
                entry(
                    "PTB",
                    "ptbtime1.ptb.de",
                    Category::Standards,
                    "Reference clocks",
                    "German national metrology institute.",
                ),
                entry(
                    "NICT",
                    "ntp.nict.jp",
                    Category::Standards,
                    "Reference clocks",
                    "Japan national time service.",
                ),
                entry(
                    "NTP Pool Project",
                    "pool.ntp.org",
                    Category::Pool,
                    "GeoDNS",
                    "Community-operated global pool.",
                ),
                // Regional pool names are preferable to hardcoded regional IPs.
                entry(
                    "North America Pool",
                    "north-america.pool.ntp.org",
                    Category::Pool,
                    "GeoDNS",
                    "Regional community pool.",
                ),
                entry(
                    "Europe Pool",
                    "europe.pool.ntp.org",
                    Category::Pool,
                    "GeoDNS",
                    "Regional community pool.",
                ),
                entry(
                    "Asia Pool",
                    "asia.pool.ntp.org",
                    Category::Pool,
                    "GeoDNS",
                    "Regional community pool.",
                ),
                entry(
                    "Oceania Pool",
                    "oceania.pool.ntp.org",
                    Category::Pool,
                    "GeoDNS",
                    "Regional community pool.",
                ),
                entry(
                    "South America Pool",
                    "south-america.pool.ntp.org",
                    Category::Pool,
                    "GeoDNS",
                    "Regional community pool.",
                ),
                entry(
                    "Africa Pool",
                    "africa.pool.ntp.org",
                    Category::Pool,
                    "GeoDNS",
                    "Regional community pool.",
                ),
                // Vendor and distribution defaults.
                entry(
                    "Ubuntu",
                    "ntp.ubuntu.com",
                    Category::Distribution,
                    "Distribution default",
                    "Ubuntu default time service.",
                ),
                entry(
                    "Fedora",
                    "2.fedora.pool.ntp.org",
                    Category::Distribution,
                    "Distribution default",
                    "Fedora regional pool alias.",
                ),
                entry(
                    "Apple",
                    "time.apple.com",
                    Category::Vendor,
                    "Anycast",
                    "Apple public time service.",
                ),
                entry(
                    "Juniper",
                    "ntp.juniper.net",
                    Category::Vendor,
                    "Vendor service",
                    "Juniper public NTP service.",
                ),
                entry(
                    "Netnod",
                    "ntp.netnod.se",
                    Category::Vendor,
                    "Vendor service",
                    "Swedish Internet infrastructure operator.",
                ),
            ],
        }
    }

    pub fn entries(&self) -> &[GlobalServerEntry] {
        &self.entries
    }

    /// Returns entries matching both the optional category and query.
    ///
    /// Search is a case-insensitive substring match against the display name,
    /// hostname, category, and descriptive metadata. An empty query matches
    /// every entry in the selected category.
    pub fn filter(&self, category: Option<Category>, query: &str) -> Vec<&GlobalServerEntry> {
        let query = query.to_lowercase();
        self.entries
            .iter()
            .filter(|entry| category.is_none_or(|wanted| entry.category == wanted))
            .filter(|entry| {
                query.is_empty()
                    || entry.profile.name().to_lowercase().contains(&query)
                    || entry.profile.hostname().to_lowercase().contains(&query)
                    || entry.category.as_str().to_lowercase().contains(&query)
                    || entry.notes.to_lowercase().contains(&query)
            })
            .collect()
    }
}

/// Convenience constructor for callers that do not need to name the type.
pub fn built_in() -> GlobalServerCatalog {
    GlobalServerCatalog::built_in()
}

fn entry(
    name: &'static str,
    hostname: &'static str,
    category: Category,
    strategy: &'static str,
    notes: &'static str,
) -> GlobalServerEntry {
    GlobalServerEntry::new(name, hostname, category, strategy, notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_entries_are_valid_and_cover_sources() {
        let catalog = GlobalServerCatalog::built_in();

        assert!(catalog.entries().len() >= 15);
        assert!(Category::ALL.iter().all(|category| {
            catalog
                .entries()
                .iter()
                .any(|entry| entry.category() == *category)
        }));
        assert!(catalog.entries().iter().all(|entry| {
            !entry.profile().name().trim().is_empty()
                && !entry.profile().hostname().trim().is_empty()
                && entry.profile().ip_fallback().is_none()
        }));
        assert!(
            catalog
                .entries()
                .iter()
                .all(|entry| entry.profile().hostname().contains('.'))
        );
    }

    #[test]
    fn filtering_limits_results_to_category() {
        let catalog = GlobalServerCatalog::built_in();
        let pools = catalog.filter(Some(Category::Pool), "");

        assert!(pools.len() >= 6);
        assert!(pools.iter().all(|entry| entry.category() == Category::Pool));
        assert!(catalog.filter(Some(Category::Standards), "pool").is_empty());
    }

    #[test]
    fn search_is_case_insensitive_and_matches_hostnames() {
        let catalog = GlobalServerCatalog::built_in();

        let cloudflare = catalog.filter(None, "CLOUDFLARE");
        assert_eq!(cloudflare.len(), 1);
        assert_eq!(cloudflare[0].profile().hostname(), "time.cloudflare.com");

        let japan = catalog.filter(None, "NTP.NICT.JP");
        assert_eq!(japan.len(), 1);
        assert_eq!(japan[0].profile().name(), "NICT");
    }
}
