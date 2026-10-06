//! Order-preserving parallel map over scoped threads.
//!
//! The result is in input order whatever the thread count. Callers that sum results do so
//! serially in that order, which is what makes a render bit-identical at any thread count.

pub(crate) fn parallel_map<T: Sync, R: Send>(
    items: &[T],
    threads: usize,
    f: impl Fn(&T) -> R + Sync,
) -> Vec<R> {
    if threads <= 1 || items.len() < 2 {
        return items.iter().map(&f).collect();
    }
    let chunk = items.len().div_ceil(threads);
    std::thread::scope(|scope| {
        let f = &f;
        let handles: Vec<_> = items
            .chunks(chunk)
            .map(|c| scope.spawn(move || c.iter().map(f).collect::<Vec<R>>()))
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("worker thread panicked"))
            .collect()
    })
}
