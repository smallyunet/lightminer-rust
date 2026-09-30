//! Decide whether a locally found nonce may be submitted.

/// Compare two difficulties without treating tiny float noise as a change.
pub(crate) fn difficulties_close(a: f64, b: f64) -> bool {
    if !a.is_finite() || !b.is_finite() {
        return false;
    }
    let scale = 1.0 + a.abs().max(b.abs());
    (a - b).abs() <= 1e-9 * scale
}

/// A share mined at `found_difficulty` meets the pool target when that
/// difficulty is at least the pool's current difficulty.
///
/// Higher difficulty is a harder (smaller) target, so a share found against
/// a harder target is still valid if the pool later lowers difficulty.
pub(crate) fn decide_share_submit(
    found_job_id: &str,
    found_difficulty: f64,
    current_job_id: Option<&str>,
    required_difficulty: f64,
) -> Result<(), String> {
    let Some(current) = current_job_id else {
        return Err(format!("stale job {found_job_id} (no active job)"));
    };
    if found_job_id != current {
        return Err(format!("stale job {found_job_id} (current {current})"));
    }
    if !found_difficulty.is_finite() || !required_difficulty.is_finite() {
        return Err("non-finite difficulty".to_string());
    }
    // A share found at difficulty D meets every target of difficulty <= D.
    if found_difficulty + 1e-9 < required_difficulty {
        return Err(format!(
            "share difficulty {found_difficulty} is below pool difficulty {required_difficulty}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn difficulties_close_ignores_float_noise() {
        assert!(difficulties_close(8.0, 8.0));
        assert!(difficulties_close(8.0, 8.0 + 1e-12));
        assert!(!difficulties_close(8.0, 9.0));
        assert!(!difficulties_close(f64::NAN, 1.0));
    }

    #[test]
    fn submit_when_job_matches_and_difficulty_is_enough() {
        assert!(decide_share_submit("job-1", 4.0, Some("job-1"), 4.0).is_ok());
        assert!(decide_share_submit("job-1", 16.0, Some("job-1"), 4.0).is_ok());
    }

    #[test]
    fn drop_stale_or_inactive_job() {
        let stale = decide_share_submit("job-old", 4.0, Some("job-new"), 4.0).unwrap_err();
        assert!(stale.contains("stale job job-old"), "{stale}");
        assert!(stale.contains("job-new"), "{stale}");

        let inactive = decide_share_submit("job-old", 4.0, None, 4.0).unwrap_err();
        assert!(inactive.contains("no active job"), "{inactive}");
    }

    #[test]
    fn drop_share_found_below_current_difficulty() {
        let reason = decide_share_submit("job-1", 1.0, Some("job-1"), 1024.0).unwrap_err();
        assert!(reason.contains("below pool difficulty"), "{reason}");
    }

    #[test]
    fn drop_non_finite_difficulty() {
        let reason = decide_share_submit("job-1", f64::NAN, Some("job-1"), 1.0).unwrap_err();
        assert!(reason.contains("non-finite"), "{reason}");
    }
}
