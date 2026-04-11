use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

use entangle_transport::{DataSegment, ReceiveError, ZeroCopyChannel};
use tracing::debug;

use crate::config::EntangleConfig;
use crate::contracts::{NodeId, ReqResQos, ZeroCopySafe};
use crate::error::{IpcError, PortError};
use crate::service::reqres::{ClientWire, ReqResShared, ServerRegistration};

/// Builder for creating a Server port.
pub struct ServerBuilder<Req: ZeroCopySafe, Res: ZeroCopySafe> {
    service_name: String,
    config: EntangleConfig,
    node_id: NodeId,
    qos: ReqResQos,
    shared: Arc<ReqResShared>,
    _marker: PhantomData<(Req, Res)>,
}

impl<Req: ZeroCopySafe, Res: ZeroCopySafe> ServerBuilder<Req, Res> {
    pub(crate) fn new(
        service_name: String,
        config: EntangleConfig,
        node_id: NodeId,
        qos: ReqResQos,
        shared: Arc<ReqResShared>,
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

    pub fn create(self) -> Result<Server<Req, Res>, IpcError> {
        let chunk_size = std::mem::size_of::<Req>()
            .max(std::mem::size_of::<Res>())
            .max(8);
        // Enough chunks for max pending requests + some headroom.
        let chunk_count = (self.qos.max_pending_requests * 2).max(16) as u32;

        let seg_name = format!(
            "entangle_{}_reqres_{:032x}",
            self.service_name.replace('/', "_"),
            self.node_id.0
        );

        let segment = Arc::new(DataSegment::create(&seg_name, chunk_size, chunk_count, 0)?);

        let clients: Arc<Mutex<Vec<ClientWire>>> = Arc::new(Mutex::new(Vec::new()));

        // Register server in shared state.
        {
            let mut wiring = self.shared.inner.lock().unwrap();
            if wiring.server.is_some() {
                return Err(crate::error::ServiceError::AlreadyExists {
                    name: self.service_name.clone(),
                }
                .into());
            }
            wiring.server = Some(ServerRegistration {
                node_id: self.node_id,
                segment: segment.clone(),
                clients: clients.clone(),
            });
        }

        debug!(service = %self.service_name, "reqres server created");

        Ok(Server {
            service_name: self.service_name,
            node_id: self.node_id,
            segment,
            clients,
            _marker: PhantomData,
        })
    }
}

/// A Server port for handling request-response communication.
pub struct Server<Req: ZeroCopySafe, Res: ZeroCopySafe> {
    service_name: String,
    node_id: NodeId,
    segment: Arc<DataSegment>,
    clients: Arc<Mutex<Vec<ClientWire>>>,
    _marker: PhantomData<(Req, Res)>,
}

impl<Req: ZeroCopySafe, Res: ZeroCopySafe> Server<Req, Res> {
    /// Receive the next request. Returns `None` if no requests pending.
    pub fn receive(&self) -> Result<Option<ActiveRequest<Req, Res>>, PortError> {
        let clients = self.clients.lock().unwrap();
        for client_wire in clients.iter() {
            match client_wire.request_channel.receive() {
                Ok(offset) => {
                    // Safety: client wrote a valid Req at this offset.
                    let request = unsafe { *self.segment.resolve_ref::<Req>(offset) };

                    // Deallocate the request chunk — we've copied the data.
                    self.segment.deallocate(offset);

                    return Ok(Some(ActiveRequest {
                        request,
                        response_channel: client_wire.response_channel.clone(),
                        segment: self.segment.clone(),
                        _marker: PhantomData,
                    }));
                }
                Err(ReceiveError::Empty) => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(None)
    }

    pub fn service_name(&self) -> &str {
        &self.service_name
    }
}

/// An active request being processed by the server.
pub struct ActiveRequest<Req: ZeroCopySafe, Res: ZeroCopySafe> {
    request: Req,
    response_channel: Arc<ZeroCopyChannel>,
    segment: Arc<DataSegment>,
    _marker: PhantomData<(Req, Res)>,
}

impl<Req: ZeroCopySafe, Res: ZeroCopySafe> ActiveRequest<Req, Res> {
    /// Get the request data.
    pub fn request(&self) -> &Req {
        &self.request
    }

    /// Send a response back to the client.
    pub fn respond(self, response: &Res) -> Result<(), PortError> {
        // Allocate a chunk for the response.
        let offset = self.segment.allocate()?;

        // Safety: just allocated, exclusive access.
        unsafe {
            let ptr = self.segment.resolve_mut::<Res>(offset);
            *ptr = *response;
        }

        // Send response offset to the client.
        self.response_channel
            .send(offset)
            .map_err(PortError::Send)?;
        Ok(())
    }
}
