//! Default-off, compile-time-only original-owner metadata failure fixtures.
//! The full application uses its normal supervisor, environment and IPC.

const POINTS: [(&str, bool); 4] = [
    (
        "observe-before",
        cfg!(feature = "blob-upload-ui-observe-before-qualification"),
    ),
    (
        "observe-after",
        cfg!(feature = "blob-upload-ui-observe-after-qualification"),
    ),
    (
        "ack-before",
        cfg!(feature = "blob-upload-ui-ack-before-qualification"),
    ),
    (
        "ack-after",
        cfg!(feature = "blob-upload-ui-ack-after-qualification"),
    ),
];

// Reject accidentally combined variants before producing any runnable program.
const _: () = assert!(
    POINTS[0].1 as u8 + POINTS[1].1 as u8 + POINTS[2].1 as u8 + POINTS[3].1 as u8 == 1,
    "select exactly one full-UI metadata qualification point"
);

pub(super) fn selected(point: &str) -> bool {
    POINTS
        .iter()
        .any(|(name, enabled)| *enabled && *name == point)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_compiled_original_metadata_point_can_fail() {
        assert_eq!(POINTS.iter().filter(|(_, enabled)| *enabled).count(), 1);
        for (point, enabled) in POINTS {
            assert_eq!(selected(point), enabled);
        }
        for point in [
            "",
            "reserve",
            "before",
            "after",
            "completion-before",
            "ack-before ",
        ] {
            assert!(!selected(point));
        }
    }
}
