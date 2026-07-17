//! A small worker pool that JPEG/PNG-encodes extracted frames and writes them to disk off the decode thread. Encoding
//! is cheap next to decoding, but doing it inline still stalls the multithreaded decoder behind each synchronous
//! encode+write; handing each frame to a worker lets the two overlap. Only the directory output uses this — the
//! in-memory and callback sinks have no per-frame encode work to offload.

use crate::error::{Error, Result};
use crate::extract::frame::ExtractedFrame;
use crate::extract::types::ImageFormat;
use std::path::PathBuf;
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

/// Cap on worker threads regardless of core count: image encoding isn't the bottleneck, so a
/// handful of workers saturates it while keeping the in-flight frame buffer (and thus memory)
/// small.
const MAX_WORKERS: usize = 8;

/// One unit of work: encode this frame and write it to this path.
type Job = (ExtractedFrame, PathBuf);

/// A pool of threads that encode-and-write frames submitted to it, fed over a bounded channel.
pub(crate) struct EncodePool {
    /// `Some` while the pool is live; taken (dropped) to close the queue on `finish`/drop.
    tx: Option<SyncSender<Job>>,
    workers: Vec<JoinHandle<()>>,
    /// The first error any worker hit, surfaced back to the producer.
    error: Arc<Mutex<Option<Error>>>,
}

impl EncodePool {
    /// Spawn a pool that encodes every submitted frame as `format` and writes it to its path.
    pub(crate) fn new(format: ImageFormat) -> Self {
        let count = thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .min(MAX_WORKERS);

        // Bounded so the decoder can't race far ahead of the encoders (backpressure + capped
        // memory): once the buffer fills, `submit` blocks until a worker frees a slot.
        let (tx, rx) = sync_channel::<Job>(count * 2);
        let rx = Arc::new(Mutex::new(rx));
        let error = Arc::new(Mutex::new(None));

        let workers = (0..count)
            .map(|_| {
                let rx = Arc::clone(&rx);
                let error = Arc::clone(&error);

                thread::spawn(move || {
                    loop {
                        // Hold the lock only to pull the next job, never while encoding.
                        let job = rx.lock().unwrap().recv();
                        let (frame, path) = match job {
                            Ok(job) => job,
                            Err(_) => break, // queue closed and drained
                        };
                        if let Err(e) = frame.into_save_as(path, format) {
                            let mut slot = error.lock().unwrap();
                            slot.get_or_insert(e);
                        }
                    }
                })
            })
            .collect();

        Self {
            tx: Some(tx),
            workers,
            error,
        }
    }

    /// Queue `frame` to be encoded and written to `path`. Returns early with the first worker
    /// error if one has already occurred, so the caller can stop feeding the pool.
    pub(crate) fn submit(&self, frame: ExtractedFrame, path: PathBuf) -> Result<()> {
        self.take_error()?;
        // A send only fails if every worker has gone; if so, surface the error they recorded.
        if self.tx.as_ref().unwrap().send((frame, path)).is_err() {
            self.take_error()?;
        }
        
        Ok(())
    }

    /// Close the queue, wait for all pending encodes+writes, then propagate the first error.
    pub(crate) fn finish(mut self) -> Result<()> {
        self.shutdown();
        self.take_error()
    }

    /// Take the first recorded worker error, if any.
    fn take_error(&self) -> Result<()> {
        match self.error.lock().unwrap().take() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Close the queue and join every worker. Idempotent: safe to call from both `finish` and
    /// `Drop` (the second call finds no sender and no workers to join).
    fn shutdown(&mut self) {
        drop(self.tx.take());
        for w in self.workers.drain(..) {
            let _ = w.join();
        }
    }
}

impl Drop for EncodePool {
    fn drop(&mut self) {
        // On the error/early-return path `finish` never ran; make sure in-flight writes still
        // complete and no worker thread is left detached.
        self.shutdown();
    }
}
