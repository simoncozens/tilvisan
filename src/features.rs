/// OpenType feature tag strings recognized by ttfautohint.
///
/// These correspond to the active `COVERAGE` entries in
/// `ttfautohint-coverages.h`, in the same order as the C `feature_tags[]`
/// array in `tafeature.c`, followed by the unconditional `dflt` entry.
pub const FEATURE_TAGS: &[&str] = &[
    "c2cp", // petite capitals from capitals
    "c2sc", // small capitals from capitals
    "ordn", // ordinals
    "pcap", // petite capitals
    "ruby", // ruby
    "sinf", // scientific inferiors
    "smcp", // small capitals
    "subs", // subscript
    "sups", // superscript
    "titl", // titling
    "dflt", // default
];

pub fn is_known_feature(tag: &str) -> bool {
    FEATURE_TAGS.contains(&tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_features_are_valid() {
        for tag in FEATURE_TAGS {
            assert!(is_known_feature(tag), "expected `{}` to be valid", tag);
        }
    }

    #[test]
    fn unknown_features_are_rejected() {
        assert!(!is_known_feature("kern"));
        assert!(!is_known_feature("liga"));
        assert!(!is_known_feature(""));
        assert!(!is_known_feature("afrc")); // commented out in coverages.h
    }
}
