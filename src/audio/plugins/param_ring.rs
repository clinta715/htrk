// Lock-free parameter transport between the main thread and the audio thread.
//
// The main thread (`ClapPluginHandle`) pushes `ParamChange`s when the user
// changes a parameter in the UI. The audio thread (`ClapPluginProcessor`)
// drains the queue before each `process()` call and feeds the values to the
// plugin as `ParamValueEvent`s.
//
// The transport is a strict single-producer / single-consumer ring from the
// `rtrb` crate, so no custom `unsafe` code is required. Only the main thread
// ever produces; parameter changes originating on the audio thread (e.g.
// sequencer automation) are held in the processor's own local queue, since
// the processor is both the producer and the consumer there.
//
// `param_id` is the CLAP `ClapId` value (a u32).
// `value` is the normalized [0.0, 1.0] plain value.

use rtrb::{Consumer, Producer, RingBuffer};

/// One parameter change request. `value` is the plugin's plain parameter
/// value (see `ParamInfo.min_value`/`max_value`).
#[derive(Debug, Clone, Copy)]
pub struct ParamChange {
    pub param_id: u32,
    pub value: f64,
}

/// Main-thread half of the parameter ring.
pub struct ParamSender {
    producer: Producer<ParamChange>,
    overflow_warned: bool,
}

impl ParamSender {
    /// Queue a change. Returns false if the ring was full and the change was
    /// dropped (a warning is emitted once per sender).
    pub fn push(&mut self, change: ParamChange) -> bool {
        match self.producer.push(change) {
            Ok(()) => {
                self.overflow_warned = false;
                true
            }
            Err(_) => {
                if !self.overflow_warned {
                    self.overflow_warned = true;
                    tracing::warn!(
                        "parameter queue full; dropped change for param {}",
                        change.param_id
                    );
                }
                false
            }
        }
    }
}

/// Audio-thread half of the parameter ring.
pub struct ParamReceiver {
    consumer: Consumer<ParamChange>,
}

impl ParamReceiver {
    /// Drain up to `count` entries into `out`, returning the number drained.
    pub fn drain_into(&mut self, out: &mut Vec<ParamChange>, count: usize) -> usize {
        let mut drained = 0;
        while drained < count {
            match self.consumer.pop() {
                Ok(change) => {
                    out.push(change);
                    drained += 1;
                }
                Err(_) => break,
            }
        }
        drained
    }
}

/// Create a connected producer/consumer pair with the requested capacity.
pub fn param_channel(capacity: usize) -> (ParamSender, ParamReceiver) {
    let (producer, consumer) = RingBuffer::new(capacity.max(2));
    (
        ParamSender {
            producer,
            overflow_warned: false,
        },
        ParamReceiver { consumer },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_channel_drains_nothing() {
        let (_tx, mut rx) = param_channel(8);
        let mut out = Vec::new();
        assert_eq!(rx.drain_into(&mut out, 100), 0);
        assert!(out.is_empty());
    }

    #[test]
    fn push_then_drain_preserves_order() {
        let (mut tx, mut rx) = param_channel(8);
        assert!(tx.push(ParamChange {
            param_id: 1,
            value: 0.5
        }));
        assert!(tx.push(ParamChange {
            param_id: 2,
            value: 0.7
        }));
        let mut out = Vec::new();
        let n = rx.drain_into(&mut out, 100);
        assert_eq!(n, 2);
        assert_eq!(out[0].param_id, 1);
        assert_eq!(out[1].param_id, 2);
    }

    #[test]
    fn full_channel_drops_new_change() {
        let (mut tx, _rx) = param_channel(2);
        assert!(tx.push(ParamChange {
            param_id: 1,
            value: 0.0
        }));
        assert!(tx.push(ParamChange {
            param_id: 2,
            value: 0.0
        }));
        assert!(!tx.push(ParamChange {
            param_id: 3,
            value: 0.0
        }));
    }

    #[test]
    fn cross_thread_visibility() {
        let (mut tx, mut rx) = param_channel(256);
        let producer = std::thread::spawn(move || {
            for i in 0..100 {
                tx.push(ParamChange {
                    param_id: i,
                    value: i as f64 / 100.0,
                });
            }
        });
        producer.join().unwrap();
        let mut out = Vec::new();
        let n = rx.drain_into(&mut out, 200);
        assert_eq!(n, 100);
        for (i, change) in out.iter().enumerate() {
            assert_eq!(change.param_id, i as u32);
        }
    }
}
