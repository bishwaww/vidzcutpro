//! Every panel the user sees (egui).
//!
//! Part of vidzcut. See docs/ARCHITECTURE.md.

/// Name of this crate, used in diagnostics.
pub const CRATE: &str = "vz-ui";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_name_is_set() {
        assert_eq!(super::CRATE, "vz-ui");
    }
}
