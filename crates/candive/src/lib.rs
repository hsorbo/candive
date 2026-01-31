#![cfg_attr(not(test), no_std)]
pub mod alerts;
#[cfg(feature = "diagnostics")]
pub mod diag;
pub mod divecan;
pub mod fmt;
#[cfg(feature = "uds")]
pub mod uds;
pub mod units;
