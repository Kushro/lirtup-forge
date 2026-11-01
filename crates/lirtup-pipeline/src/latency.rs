//! Latency CSV, one row per presented frame
//! (`capture_s,present_s,latency_ms,kind,multiplier`; `LatencyLog` in
//! `stats.py`). Front S, round 2.
//!
//! Only [`crate::stats::Stats`] writes it; the content must match 1.x row
//! for row (criterion of 5.8, round 2).

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::Result;
use crate::stats::PresentRecord;

/// The header line, as 1.x writes it.
pub const HEADER: &str = "capture_s,present_s,latency_ms,kind,multiplier";

/// Rows reach the disk about this often (1.x: once a second).
const FLUSH_EVERY: Duration = Duration::from_secs(1);

/// An open latency CSV.
pub struct LatencyLog {
    file: BufWriter<File>,
    origin: Instant,
    last_flush: Instant,
}

impl LatencyLog {
    /// Create (truncate) `path` and write the header. Times in the rows are
    /// seconds since `origin` (the session start).
    pub fn create(path: &Path, origin: Instant) -> Result<LatencyLog> {
        let mut file = BufWriter::new(File::create(path)?);
        writeln!(file, "{HEADER}")?;
        Ok(LatencyLog {
            file,
            origin,
            last_flush: origin,
        })
    }

    /// One row. Records without a capture time are skipped (no latency).
    pub fn write(&mut self, record: &PresentRecord) -> Result<()> {
        let Some(capture) = record.capture else {
            return Ok(());
        };
        let line = row(
            self.seconds(capture),
            self.seconds(record.presented),
            record.kind.as_str(),
            record.multiplier,
        );
        self.file.write_all(line.as_bytes())?;
        if record.presented.saturating_duration_since(self.last_flush) >= FLUSH_EVERY {
            self.flush()?;
            self.last_flush = record.presented;
        }
        Ok(())
    }

    /// Flush to disk.
    pub fn flush(&mut self) -> Result<()> {
        self.file.flush()?;
        Ok(())
    }

    /// Seconds since the origin; negative for an instant before it.
    fn seconds(&self, at: Instant) -> f64 {
        match at.checked_duration_since(self.origin) {
            Some(d) => d.as_secs_f64(),
            None => -self.origin.duration_since(at).as_secs_f64(),
        }
    }
}

/// One CSV row with its newline, formatted as 1.x
/// (`{capture:.6f},{present:.6f},{latency:.3f},{kind},{multiplier}`).
fn row(capture_s: f64, present_s: f64, kind: &str, multiplier: u8) -> String {
    let latency_ms = (present_s - capture_s) * 1000.0;
    format!("{capture_s:.6},{present_s:.6},{latency_ms:.3},{kind},{multiplier}\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::FrameKind;

    #[test]
    fn rows_match_the_1x_format() {
        assert_eq!(
            row(0.95, 1.0, "generated", 2),
            "0.950000,1.000000,50.000,generated,2\n"
        );
        assert_eq!(row(1.5, 1.5125, "real", 1), "1.500000,1.512500,12.500,real,1\n");
    }

    #[test]
    fn writes_header_and_rows_and_skips_repeats() {
        let dir = tempdir();
        let path = dir.join("latency.csv");
        let origin = Instant::now();
        let mut log = LatencyLog::create(&path, origin).unwrap();
        let capture = origin + Duration::from_millis(950);
        for (presented, kind) in [(1000, FrameKind::Generated), (1010, FrameKind::Real)] {
            log.write(&PresentRecord {
                capture: Some(capture),
                presented: origin + Duration::from_millis(presented),
                kind,
                multiplier: 2,
            })
            .unwrap();
        }
        log.write(&PresentRecord {
            capture: None,
            presented: origin + Duration::from_millis(1020),
            kind: FrameKind::Real,
            multiplier: 2,
        })
        .unwrap();
        log.flush().unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], HEADER);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1], "0.950000,1.000000,50.000,generated,2");
        assert_eq!(lines[2], "0.950000,1.010000,60.000,real,2");
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn tempdir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("lirtup-latency-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
