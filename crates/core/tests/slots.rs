//! The requests in progress (spec 022 R5.2): at most `MAX_WORKERS` at once.

use solite_core::api::{MAX_WORKERS, Slots};

#[test]
fn a_fifth_request_is_refused_until_a_slot_is_released() {
    assert_eq!(MAX_WORKERS, 4);
    let slots = Slots::new(MAX_WORKERS);
    let mut guards: Vec<_> = (0..MAX_WORKERS)
        .map(|_| slots.try_take().expect("a free slot"))
        .collect();
    assert!(slots.try_take().is_none(), "the 5th is refused");
    guards.pop();
    let again = slots.try_take();
    assert!(again.is_some(), "one dropped, the next is taken");
    assert!(slots.try_take().is_none());
}

#[test]
fn a_guard_dropped_while_a_panic_unwinds_releases_its_slot() {
    let slots = Slots::new(1);
    let worker = slots.clone();
    let result = std::thread::spawn(move || {
        let _guard = worker.try_take().expect("a free slot");
        panic!("a worker panics while it holds the slot");
    })
    .join();
    assert!(result.is_err());
    assert!(slots.try_take().is_some(), "the slot was released");
}
