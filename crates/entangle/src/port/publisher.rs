use std::marker::PhantomData;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use entangle_transport::{DataSegment, ZeroCopyChannel};
use tracing::debug;

use crate::config::EntangleConfig;
use crate::contracts::{NodeId, PubSubQos, ZeroCopySafe};
use crate::error::{IpcError, PortError};
use crate::sample::SampleMut;
use crate::service::pubsub::{PubSubShared, PublisherRegistration};

/// Builder for creating a Publisher port.
pub struct PublisherBuilder<T: ZeroCopySafe> {
    service_name: String,
    config: EntangleConfig,
    node_id: NodeId,
    qos: PubSubQos,
    shared: Arc<PubSubShared>,
    _marker: PhantomData<T>,
}

impl<T: ZeroCopySafe> PublisherBuilder<T> {
    pub(crate) fn new(
        service_name: String,
        config: EntangleConfig,
        node_id: NodeId,
        qos: PubSubQos,
        shared: Arc<PubSubShared>,
    ) -> Self {
        Self {
            service_name,
            config,
            node_id,
            qos,
            shared,
            _marker: PhantomData,
        }
    }

    /// Create the publisher.
    pub fn create(self) -> Result<Publisher<T>, IpcError> {
        let chunk_size = std::mem::size_of::<T>().max(8);
        let chunk_count = self.qos.buffer_size.get() as u32;

        let pub_idx = self
            .shared
            .publisher_counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let seg_name = format!(
            "entangle_{}_p{}_{:016x}",
            self.service_name.replace('/', "_"),
            pub_idx,
            self.node_id.0 as u64, // lower 64 bits for shorter name
        );

        let segment = Arc::new(DataSegment::create(&seg_name, chunk_size, chunk_count, 0)?);

        let channels: Arc<Mutex<Vec<Arc<ZeroCopyChannel>>>> = Arc::new(Mutex::new(Vec::new()));

        // Register this publisher in the shared state so subscribers can connect.
        {
            let mut wiring = self.shared.inner.lock().unwrap();
            wiring.publishers.push(PublisherRegistration {
                node_id: self.node_id,
                segment: segment.clone(),
                channels: channels.clone(),
            });
        }

        debug!(
            service = %self.service_name,
            chunk_size,
            chunk_count,
            "publisher created"
        );

        Ok(Publisher {
            service_name: self.service_name,
            node_id: self.node_id,
            segment,
            channels,
            max_loaned: self.qos.max_loaned_samples,
            active_loans: Arc::new(AtomicUsize::new(0)),
            _marker: PhantomData,
        })
    }
}

/// A Publisher port for zero-copy data publishing.
///
/// Usage: `loan()` -> write data -> `send()`. Only a pointer offset is
/// transmitted to subscribers — zero bytes are copied.
pub struct Publisher<T: ZeroCopySafe> {
    service_name: String,
    node_id: NodeId,
    segment: Arc<DataSegment>,
    /// Channels to subscribers. Shared with PubSubShared — new subscribers
    /// add channels to this list dynamically.
    channels: Arc<Mutex<Vec<Arc<ZeroCopyChannel>>>>,
    max_loaned: usize,
    active_loans: Arc<AtomicUsize>,
    _marker: PhantomData<T>,
}

impl<T: ZeroCopySafe> Publisher<T> {
    /// Borrow a slot from shared memory for writing.
    ///
    /// Returns a `SampleMut<T>` that can be written to via `DerefMut`.
    /// Call `.send()` on the sample to publish it, or drop it to return
    /// the slot without sending.
    pub fn loan(&mut self) -> Result<SampleMut<T>, PortError> {
        let current = self.active_loans.load(Ordering::Relaxed);
        if current >= self.max_loaned {
            return Err(entangle_transport::LoanError::ExceedsMaxLoans {
                max: self.max_loaned,
            }
            .into());
        }

        let offset = self.segment.allocate()?;
        self.active_loans.fetch_add(1, Ordering::Relaxed);

        // Safety: offset was just allocated, we have exclusive access
        let ptr = unsafe { self.segment.resolve_ptr(offset) };

        // Zero-initialize the payload
        unsafe {
            std::ptr::write_bytes(ptr.as_ptr(), 0, std::mem::size_of::<T>());
        }

        // Capture Arcs for the closures — keeps DataSegment alive as long
        // as any SampleMut referencing it exists.
        let channels_for_send = self.channels.clone();
        let segment_for_drop = self.segment.clone();
        let loans_for_drop = self.active_loans.clone();

        // Safety: SampleMut ensures these closures are called correctly.
        // The Arc<DataSegment> in the closure keeps the segment alive,
        // preventing use-after-free on the raw pointer.
        let sample = unsafe {
            SampleMut::new(
                ptr.as_ptr() as *mut T,
                offset,
                move |offset| {
                    // Push the offset to all subscriber channels.
                    let channels = channels_for_send.lock().unwrap();
                    for ch in channels.iter() {
                        ch.send(offset).map_err(PortError::Send)?;
                    }
                    Ok(())
                },
                move |offset| {
                    // send() was not called — return slot to pool and decrement loans.
                    segment_for_drop.deallocate(offset);
                    loans_for_drop.fetch_sub(1, Ordering::Relaxed);
                },
            )
        };

        Ok(sample)
    }

    /// Reclaim returned slots from subscribers.
    ///
    /// Subscribers return consumed offsets via the return channel.
    /// This method deallocates those slots so they can be reused.
    pub fn reclaim(&self) {
        let channels = self.channels.lock().unwrap();
        for channel in channels.iter() {
            while let Some(offset) = channel.reclaim() {
                self.segment.deallocate(offset);
                self.active_loans.fetch_sub(1, Ordering::Relaxed);
            }
        }
    }

    /// Number of active loans (allocated but not yet reclaimed).
    pub fn active_loans(&self) -> usize {
        self.active_loans.load(Ordering::Relaxed)
    }

    /// Service name.
    pub fn service_name(&self) -> &str {
        &self.service_name
    }
}
