//! Size expressions of `//!WIDTH` / `//!HEIGHT` (`expr.py`):
//! `INPUT_WIDTH * 2`, `OUTPUT_HEIGHT / 4`, with `+ - * / ( )`, comparisons
//! and `?:`.

use std::collections::BTreeMap;

use crate::Result;

/// Evaluate `text` with the given variables (`INPUT_WIDTH`, ...).
pub fn evaluate(text: &str, variables: &BTreeMap<String, f64>) -> Result<f64> {
    todo!("front E")
}
