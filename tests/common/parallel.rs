//! Corpus loops on every core: the corpus tests are one `#[test]` each (one list of problems,
//! one stale-entry check), so they spread their per-file work over threads themselves.

use std::sync::atomic::{AtomicUsize, Ordering};

/// `items.iter().map(f)`, run on `std::thread::available_parallelism` threads, each taking the
/// next unclaimed item (corpus files differ a hundredfold in cost, so fixed chunks would leave
/// threads idle). Results come back in `items`' order, so what a test prints and the order of its
/// failure messages do not depend on scheduling. A panic in `f` fails the caller.
pub fn map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(items.len().max(1));
    let next = AtomicUsize::new(0);
    let mut done: Vec<(usize, R)> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| {
                    let mut out = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(i) else { break };
                        out.push((i, f(item)));
                    }
                    out
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|w| w.join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
            .collect()
    });
    done.sort_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, r)| r).collect()
}
