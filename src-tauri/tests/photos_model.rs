use mediacenter_lib::photos::model::*;
fn request() -> SlideshowRequest {
    SlideshowRequest {
        media_ids: vec!["a".into(), "b".into(), "c".into()],
        monitor_id: None,
        interval_seconds: 5,
        shuffle: false,
    }
}
#[test]
fn ordered_navigation_pause_resume_and_release() {
    let mut q = Queue::new(request()).unwrap();
    assert_eq!(q.current(), "a");
    q.next();
    assert_eq!(q.current(), "b");
    q.previous();
    assert_eq!(q.current(), "a");
    q.previous();
    assert_eq!(q.current(), "c");
    assert_eq!(q.preloads(), vec!["a", "b"]);
    q.pause(true);
    assert!(q.paused);
    q.pause(false);
    assert!(!q.paused);
}
#[test]
fn bounds_reject_empty_oversize_invalid_interval_and_accept_single_photo() {
    for interval in [0, 61, u32::MAX] {
        let mut r = request();
        r.interval_seconds = interval;
        assert!(Queue::new(r).is_err());
    }
    let mut r = request();
    r.media_ids.clear();
    assert!(Queue::new(r).is_err());
    let mut r = request();
    r.media_ids = vec!["a".into(); 10_001];
    assert!(Queue::new(r).is_err());
    let mut r = request();
    r.media_ids = vec!["one".into()];
    assert!(Queue::new(r).unwrap().preloads().is_empty());
}
#[test]
fn shuffle_preserves_members_and_restores_original_order() {
    let mut q = Queue::new(request()).unwrap();
    q.shuffle(true);
    let mut actual = q.ids.clone();
    actual.sort();
    assert_eq!(actual, vec!["a", "b", "c"]);
    q.shuffle(false);
    assert_eq!(q.ids, vec!["a", "b", "c"]);
}
#[test]
fn omitted_options_default_to_five_seconds_and_source_order() {
    let request: SlideshowRequest =
        serde_json::from_value(serde_json::json!({"mediaIds":["first","second"]})).unwrap();
    let queue = Queue::new(request).unwrap();
    assert_eq!(queue.interval, 5);
    assert!(!queue.shuffled);
    assert_eq!(queue.ids, vec!["first", "second"]);
}
