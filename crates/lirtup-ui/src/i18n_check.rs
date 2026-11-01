//! The test that both languages are complete (round 4, front M): every key
//! the code names exists in `es` and `en`, both define the same keys, and
//! none is defined twice (what `scripts/check-ftl-keys.py` checks today, as
//! a `cargo test`). Uses `i18n::has_key`.
