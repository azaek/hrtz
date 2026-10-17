//! When a mouse was last used on each USB port, from raw input (`meter`). A mouse that sleeps
//! behind its receiver is told awake by this, not by asking it: asking can wake it, or catch it as
//! it checks in, and keep it from sleeping.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Instant;

use parking_lot::Mutex;

static LAST: LazyLock<Mutex<HashMap<String, Instant>>> = LazyLock::new(Mutex::default);

/// A mouse on `port` moved or was clicked just now.
pub fn note(port: &str) {
    let now = Instant::now();
    let mut last = LAST.lock();
    match last.get_mut(port) {
        Some(t) => *t = now,
        None => {
            last.insert(port.to_string(), now);
        }
    }
}

/// Whether a mouse on `port` was used after `t`.
pub fn since(port: &str, t: Instant) -> bool {
    LAST.lock().get(port).is_some_and(|last| *last > t)
}
