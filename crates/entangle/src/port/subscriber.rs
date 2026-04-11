use std::marker::PhantomData;
use std::sync::Arc;

use entangle_transport::{DataSegment, ReceiveError, ZeroCopyChannel};
use tracing::debug;

use crate::config::EntangleConfig;
use crate::contracts::{NodeId, PubSubQos, ZeroCopySafe};
use crate::error::{IpcError, PortError};
use crate::sample::Sample;
use crate::service::pubsub::PubSubShared;

/// A connection from this subscriber to one publisher.
pub(crate) struct SubscriberConnection {
    pub(crate) segment: Arc<DataSegment>,
    pub(crate) channel: Arc<ZeroCopyChannel>,
}

/// Builder for creating a Subscriber port.
pub struct SubscriberBuilder<T: ZeroCopySafe> {
    service_name: String,
    config: EntangleConfig,
    node_id: NodeId,
    qos: PubSubQos,
    shared: Arc<PubSubShared>,
    _marker: PhantomData<T>,
}

impl<T: ZeroCopySafe> SubscriberBuilder<T> {
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

    /// Create the subscriber.
    ///
    /// Connects to all currently registered publishers. Publishers created
    /// after this point will not be automatically discovered.
    pub fn create(self) -> Result<Subscriber<T>, IpcError> {
        let mut connections = Vec::new();

        // Lock shared state and establish channels with all existing publishers.
        {
            let mut wiring = self.shared.inner.lock().unwrap();
            let buffer_size = self.qos.buffer_size.get();

            for pub_reg in &mut wiring.publishers {
                let channel = Arc::new(ZeroCopyChannel::new(
                    buffer_size,
                    pub_reg.node_id.0,
                    self.node_id.0,
                ));
                channel.connect();

                // Add to the publisher's channel list so it sends to us.
                pub_reg.channels.lock().unwrap().push(channel.clone());

                connections.push(SubscriberConnection {
                    segment: pub_reg.segment.clone(),
                    channel,
                });
            }
        }

        debug!(
            service = %self.service_name,
            publishers = connections.len(),
            "subscriber created"
        );

        Ok(Subscriber {
            service_name: self.service_name,
            node_id: self.node_id,
            connections,
            _marker: PhantomData,
        })
    }
}

/// A Subscriber port for zero-copy data receiving.
///
/// Receives `Sample<T>` from publishers. The sample points directly
/// into the publisher's shared memory — zero copies.
pub struct Subscriber<T: ZeroCopySafe> {
    service_name: String,
    node_id: NodeId,
    connections: Vec<SubscriberConnection>,
    _marker: PhantomData<T>,
}

impl<T: ZeroCopySafe> Subscriber<T> {
    /// Receive a sample. Returns `None` if no data is available.
    ///
    /// The returned `Sample<T>` holds a reference to shared memory.
    /// When dropped, the underlying slot is returned to the publisher
    /// via the return channel.
    pub fn receive(&self) -> Result<Option<Sample<T>>, PortError> {
        for conn in &self.connections {
            match conn.channel.receive() {
                Ok(offset) => {
                    // Safety: the offset was allocated by the publisher from this segment.
                    // The publisher's DataSegment is kept alive by Arc in the closure.
                    let ptr = unsafe { conn.segment.resolve_ptr(offset) };

                    let channel = conn.channel.clone();
                    let sample = unsafe {
                        Sample::new(ptr.as_ptr() as *const T, offset, move |offset| {
                            // Return offset to publisher for reclamation.
                            channel.return_offset(offset);
                        })
                    };
                    return Ok(Some(sample));
                }
                Err(ReceiveError::Empty) => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(None)
    }

    /// Number of connected publishers.
    pub fn publisher_count(&self) -> usize {
        self.connections.len()
    }

    /// Service name.
    pub fn service_name(&self) -> &str {
        &self.service_name
    }
}
