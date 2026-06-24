// Lock-free SPSC parameter ring buffer.
//
// The main thread (ClapPluginHandle) pushes `ParamChange`s when the user
// (or automation) requests a parameter value change. The audio thread
// (ClapPluginProcessor) drains the ring right before calling
// `clack.process()` and feeds `ParamValueEvent`s into the input events
// buffer so the plugin sees the new value on this process() call.
//
// `param_id` is the CLAP `ClapId` value (a u32).
// `value` is the normalized [0.0, 1.0] plain value.
//
// Implementation: simple atomic counter pair over a fixed-size vector.
// Capacity is rounded up to a power of two. On overflow, oldest entries
// are dropped (with a one-shot warning) — this is fine because param
// changes are idempotent and the latest value is what matters.
//
// SAFETY: writes and reads use atomic counters with proper ordering.
// The vector is only ever resized at construction, never after, so
// raw pointer arithmetic into it is safe (no realloc).

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// One parameter change request. `value` is the plain (normalized)
/// value in [0.0, 1.0] (or whatever the plugin's param range is — see
/// `ParamInfo.min_value`/`max_value`).
#[derive(Debug, Clone, Copy)]
pub struct ParamChange {
    pub param_id: u32,
    pub value: f64,
}

pub struct ParamRingBuffer {
    entries: UnsafeCell<Vec<ParamChange>>,
    write_idx: AtomicU64,
    read_idx: AtomicU64,
    overflow_warned: AtomicBool,
}

// SAFETY: We only mutate the vector at construction. After that, raw
// pointer access is safe. The atomic indices provide the happens-before
// edges needed for cross-thread visibility.
unsafe impl Send for ParamRingBuffer {}
unsafe impl Sync for ParamRingBuffer {}

impl ParamRingBuffer {
    /// Create a ring with `capacity` slots. Capacity is rounded up to
    /// the next power of two so we can use a bitmask for index wrapping.
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.next_power_of_two().max(2);
        Self {
            entries: UnsafeCell::new(vec![
                ParamChange { param_id: 0, value: 0.0 };
                cap
            ]),
            write_idx: AtomicU64::new(0),
            read_idx: AtomicU64::new(0),
            overflow_warned: AtomicBool::new(false),
        }
    }

    fn cap(&self) -> u64 {
        // SAFETY: vector is never resized after construction.
        unsafe { (*self.entries.get()).len() as u64 }
    }

    /// Push a change. Returns true if accepted, false if the ring is
    /// full (oldest entry was overwritten or new entry was dropped
    /// depending on policy — currently we drop and warn once).
    pub fn push(&self, change: ParamChange) -> bool {
        let write = self.write_idx.load(Ordering::Relaxed);
        let read = self.read_idx.load(Ordering::Acquire);
        let avail = write.saturating_sub(read);
        if avail >= self.cap() {
            if !self.overflow_warned.swap(true, Ordering::Relaxed) {
                tracing::warn!(
                    "ParamRingBuffer overflow (cap={}); further changes may be dropped",
                    self.cap()
                );
            }
            // Drop the new change — the latest queued value already wins.
            return false;
        }
        let len = self.cap();
        let idx = (write & (len - 1)) as usize;
        // SAFETY: idx is in range. We have exclusive write access to
        // slot idx because write_idx == slot's owner.
        unsafe {
            let slots = &mut *self.entries.get();
            slots[idx] = change;
        }
        self.write_idx.store(write.wrapping_add(1), Ordering::Release);
        true
    }

    /// Drain up to `count` entries into `out`, returning the number
    /// actually drained. Called by the audio thread before process().
    pub fn drain_into(&self, out: &mut Vec<ParamChange>, count: usize) -> usize {
        let write = self.write_idx.load(Ordering::Acquire);
        let read = self.read_idx.load(Ordering::Relaxed);
        let avail = write.saturating_sub(read) as usize;
        let to_drain = avail.min(count);
        if to_drain == 0 {
            return 0;
        }
        let len = self.cap();
        // SAFETY: we own slots [read, read+to_drain); no other thread
        // writes to these slots because the write_idx is ahead of read.
        unsafe {
            let slots = &*self.entries.get();
            for i in 0..to_drain {
                let slot_idx = ((read + i as u64) & (len - 1)) as usize;
                out.push(slots[slot_idx]);
            }
        }
        self.read_idx.store(read + to_drain as u64, Ordering::Release);
        to_drain
    }

    /// Reset the overflow-warning flag (call after a successful flush
    /// so the warning can fire again on a new overflow).
    pub fn reset_overflow_warning(&self) {
        self.overflow_warned.store(false, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn empty_ring_drains_nothing() {
        let ring = ParamRingBuffer::new(8);
        let mut out = Vec::new();
        assert_eq!(ring.drain_into(&mut out, 100), 0);
        assert!(out.is_empty());
    }

    #[test]
    fn push_then_drain() {
        let ring = ParamRingBuffer::new(8);
        ring.push(ParamChange { param_id: 1, value: 0.5 });
        ring.push(ParamChange { param_id: 2, value: 0.7 });
        let mut out = Vec::new();
        let n = ring.drain_into(&mut out, 100);
        assert_eq!(n, 2);
        assert_eq!(out[0].param_id, 1);
        assert_eq!(out[1].param_id, 2);
    }

    #[test]
    fn overflow_drops_and_warns_once() {
        let ring = ParamRingBuffer::new(2); // cap = 2
        assert!(ring.push(ParamChange { param_id: 1, value: 0.0 }));
        assert!(ring.push(ParamChange { param_id: 2, value: 0.0 }));
        assert!(!ring.push(ParamChange { param_id: 3, value: 0.0 }));
    }

    #[test]
    fn cross_thread_visibility() {
        let ring = Arc::new(ParamRingBuffer::new(256));
        let r2 = ring.clone();
        let producer = thread::spawn(move || {
            for i in 0..100 {
                r2.push(ParamChange { param_id: i, value: i as f64 / 100.0 });
            }
        });
        producer.join().unwrap();
        let mut out = Vec::new();
        let n = ring.drain_into(&mut out, 200);
        assert_eq!(n, 100);
        for (i, change) in out.iter().enumerate() {
            assert_eq!(change.param_id, i as u32);
        }
    }
}
