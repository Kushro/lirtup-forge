//! Latency CSV, one row per presented frame
//! (`capture_s,present_s,latency_ms,kind,multiplier`; `LatencyLog` in
//! `stats.py`). Front S, round 2.
//!
//! Only [`crate::stats::Stats`] writes it; the content must match 1.x row
//! for row (criterion of 5.8, round 2).

use std::path::Path;
use std::time::Instant;

use crate::Result;
use crate::stats::PresentRecord;

/// The header line, as 1.x writes it.
pub const HEADER: &str = "capture_s,present_s,latency_ms,kind,multiplier";

/// An open latency CSV.
pub struct LatencyLog {
    _private: (),
}

impl LatencyLog {
    /// Create (truncate) `path` and write the header. Times in the rows are
    /// seconds since `origin` (the session start).
    pub fn create(path: &Path, origin: Instant) -> Result<LatencyLog> {
        todo!("front S")
    }

    /// One row. Records without a capture time are skipped (no latency).
    pub fn write(&mut self, record: &PresentRecord) -> Result<()> {
        todo!("front S")
    }

    /// Flush to disk.
    pub fn flush(&mut self) -> Result<()> {
        todo!("front S")
    }
}
