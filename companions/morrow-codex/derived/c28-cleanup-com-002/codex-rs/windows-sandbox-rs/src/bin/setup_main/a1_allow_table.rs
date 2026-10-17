//! S-only input. P first, final C/R next, S last. No self digest is embedded.
//! ROOT replaces only this S source input with 80 exact bytes after freezing
//! C/R. Layout is C28A1TAB/version1/count1/C-SHA256/R-SHA256 (16+32+32).
//! Zero/default is intentionally unbound; never infer authority from JSON/env.
#[used]
#[unsafe(link_section = ".c28a1")]
pub(super) static TABLE: [u8; 80] = [0; 80];
