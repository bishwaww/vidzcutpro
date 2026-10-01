//! Built-in effects, transitions, filters, animations and text styles, stored as data files.
//!
//! Part of vidzcut. See docs/ARCHITECTURE.md.

/// Name of this crate, used in diagnostics.
pub const CRATE: &str = "vz-presets";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_name_is_set() {
        assert_eq!(super::CRATE, "vz-presets");
    }
}
