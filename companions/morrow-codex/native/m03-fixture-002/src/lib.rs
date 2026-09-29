//! Native v3 integration work in progress. Wire source is the host's sealed kit.
//! Pure state code is not a native grant, pipe, or HTTP success implementation.
#![forbid(unsafe_code)]
pub mod admission;
pub mod delivery;
pub mod session;
pub mod workers;
