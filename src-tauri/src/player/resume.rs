pub fn position(loaded: bool, saved: f64, duration: Option<f64>, completed: bool) -> Option<f64> {
    if !loaded || completed || !saved.is_finite() || saved < 10. {
        return None;
    }
    if let Some(duration) = duration.filter(|d| d.is_finite() && *d > 0.) {
        if saved >= duration {
            return (duration >= 20.).then_some(duration - 10.);
        }
        if saved >= duration * 0.95 {
            return None;
        }
    }
    Some(saved)
}
pub fn completed(position: f64, duration: Option<f64>, eof: bool) -> bool {
    eof || duration.is_some_and(|d| d.is_finite() && d > 0. && position >= d * 0.95)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resume_waits_for_file_loaded() {
        assert_eq!(position(false, 30., Some(100.), false), None);
        assert_eq!(position(true, 30., Some(100.), false), Some(30.));
    }
    #[test]
    fn completed_video_does_not_resume_near_end() {
        assert_eq!(position(true, 96., Some(100.), false), None);
        assert!(completed(96., Some(100.), false));
        assert!(completed(10., None, true));
        assert_eq!(position(true, 30., Some(100.), true), None);
    }
    #[test]
    fn changed_duration_clamps_resume() {
        let result = position(true, 180., Some(100.), false).unwrap();
        assert!((10.0..100.0).contains(&result));
        assert_eq!(position(true, f64::NAN, Some(100.), false), None);
    }
}
