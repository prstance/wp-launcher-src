//! Version ranges as the packages write them ("*", ">=1.2"), and the one each package gives for the
//! launcher ("launcher"), which decides whether this launcher reads it at all. Also used by
//! examples/wplink.rs, which refuses the same packages.

/// The first launcher whose packages this one reads: core v2's own API (the board's save in the core,
/// `wpBoardSave`, not in the manifest). Every package says `"launcher": ">=0.6.0"` or more: one that asks
/// for less, or for nothing, was made for an older core and is refused. While Cargo.toml's version is
/// older, the ranges are checked against this one too.
pub const READS_PACKAGES_OF: &str = "0.6.0";

/// What a package's "launcher" range tells this launcher.
#[derive(Debug, PartialEq)]
pub enum Launcher {
    /// It reads the package.
    Reads,
    /// Made for an older core (and launcher): its mod has to be updated.
    Old,
    /// It asks for a newer launcher.
    Newer,
    /// Not a range.
    Invalid,
}

fn numbers(version: &str) -> Vec<u64> {
    version.split('.').map(|n| n.trim().parse().unwrap_or(0)).collect()
}

pub fn range_is_valid(range: &str) -> bool {
    range == "*" || range.strip_prefix(">=").is_some_and(|v| v.split('.').all(|n| n.trim().parse::<u64>().is_ok()))
}

pub fn satisfies(version: &str, range: &str) -> bool {
    let Some(least) = range.strip_prefix(">=") else { return true };
    let (mut have, mut want) = (numbers(version), numbers(least));
    let len = have.len().max(want.len());
    have.resize(len, 0);
    want.resize(len, 0);
    have >= want
}

/// The version a package's "launcher" range is checked against: Cargo.toml's, at least READS_PACKAGES_OF.
fn launcher_version() -> &'static str {
    let own = env!("CARGO_PKG_VERSION");
    if satisfies(own, &format!(">={READS_PACKAGES_OF}")) { own } else { READS_PACKAGES_OF }
}

/// A package's "launcher" range (none: an older package, from before every package had one).
pub fn launcher(range: Option<&str>) -> Launcher {
    let Some(range) = range else { return Launcher::Old };
    if !range_is_valid(range) {
        return Launcher::Invalid;
    }
    if !satisfies(launcher_version(), range) {
        return Launcher::Newer;
    }
    // ("*" asks for nothing: as old as no range)
    let least = range.strip_prefix(">=").unwrap_or("0");
    if satisfies(least, &format!(">={READS_PACKAGES_OF}")) { Launcher::Reads } else { Launcher::Old }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(satisfies("1.2.0", ">=1.2"));
        assert!(satisfies("1.10", ">=1.2"));
        assert!(!satisfies("1.1.9", ">=1.2"));
        assert!(satisfies("0.1", "*"));
    }

    #[test]
    fn reads_the_packages_of_this_core_only() {
        assert_eq!(launcher(Some(">=0.6.0")), Launcher::Reads);
        assert_eq!(launcher(Some(">=0.6")), Launcher::Reads);
        for old in [Some(">=0.5"), Some(">=0.5.1"), Some(">=0.2"), Some("*"), None] {
            assert_eq!(launcher(old), Launcher::Old, "{old:?}");
        }
        assert_eq!(launcher(Some(">=99")), Launcher::Newer);
        assert_eq!(launcher(Some("~1")), Launcher::Invalid);
    }
}
