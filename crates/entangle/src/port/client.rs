use std::marker::PhantomData;
use std::sync::Arc;

use entangle_transport::{DataSegment, ReceiveError, ZeroCopyChannel};

use crate::contracts::{NodeId, ReqResQos, ZeroCopySafe};
use crate::error::{IpcError, PortError};
use crate::service::reqres::{ClientWire, ReqResShared};

/// Builder for creating a Client port.
pub struct ClientBuilder<Req: ZeroCopySafe, Res: ZeroCopySafe> {
    service_name: String,
    node_id: NodeId,
    qos: ReqResQos,
    shared: Arc<ReqResShared>,
    _marker: PhantomData<(Req, Res)>,
}

impl<Req: ZeroCopySafe, Res: ZeroCopySafe> ClientBuilder<Req, Res> {
    pub(crate) fn new(
        service_name: String,
        node_id: NodeId,
        qos: ReqResQos,
        shared: Arc<ReqResShared>,
    ) -> Self {
        Self {
            service_name,
            node_id,
            qos,
            shared,
            _marker: PhantomData,
        }
    }

    pub fn create(self) -> Result<Client<Req, Res>, IpcError> {
        let wiring = self.shared.inner.lock().unwrap();
        let server_reg =
            wiring
                .server
                .as_ref()
                .ok_or_else(|| crate::error::ServiceError::NotFound {
                    name: self.service_name.clone(),
                })?;

        let capacity = self.qos.max_pending_requests;

        // request_channel: client is sender (pushes request offsets to server)
        let request_channel = Arc::new(ZeroCopyChannel::new(
            capacity,
            self.node_id.0,
            server_reg.node_id.0,
        ));
        request_channel.connect();

        // response_channel: server is sender (pushes response offsets to client)
        let response_channel = Arc::new(ZeroCopyChannel::new(
            capacity,
            server_reg.node_id.0,
            self.node_id.0,
        ));
        response_channel.connect();

        let segment = server_reg.segment.clone();

        // Register this client with the server.
        server_reg.clients.lock().unwrap().push(ClientWire {
            client_id: self.node_id,
            request_channel: request_channel.clone(),
            response_channel: response_channel.clone(),
        });

        Ok(Client {
            service_name: self.service_name,
            node_id: self.node_id,
            segment,
            request_channel,
            response_channel,
            _marker: PhantomData,
        })
    }
}

/// A Client port for request-response communication.
///
/// Sends requests and waits for responses. Both request and response
/// data are transferred via shared memory (zero copy).
pub struct Client<Req: ZeroCopySafe, Res: ZeroCopySafe> {
    service_name: String,
    node_id: NodeId,
    segment: Arc<DataSegment>,
    request_channel: Arc<ZeroCopyChannel>,
    response_channel: Arc<ZeroCopyChannel>,
    _marker: PhantomData<(Req, Res)>,
}

impl<Req: ZeroCopySafe, Res: ZeroCopySafe> Client<Req, Res> {
    /// Send a request and get a pending response handle.
    pub fn send(&self, request: &Req) -> Result<PendingResponse<Res>, PortError> {
        // Allocate a chunk for the request data.
        let offset = self.segment.allocate()?;

        // Safety: offset just allocated, exclusive access.
        unsafe {
            let ptr = self.segment.resolve_mut::<Req>(offset);
            *ptr = *request;
        }

        // Send the request offset to the server.
        self.request_channel.send(offset).map_err(PortError::Send)?;

        Ok(PendingResponse {
            segment: self.segment.clone(),
            response_channel: self.response_channel.clone(),
            _marker: PhantomData,
        })
    }

    pub fn service_name(&self) -> &str {
        &self.service_name
    }
}

/// Handle for a pending response.
pub struct PendingResponse<Res: ZeroCopySafe> {
    segment: Arc<DataSegment>,
    response_channel: Arc<ZeroCopyChannel>,
    _marker: PhantomData<Res>,
}

impl<Res: ZeroCopySafe> PendingResponse<Res> {
    /// Try to receive a response without blocking.
    /// Returns `None` if the response is not yet available.
    pub fn try_receive(&self) -> Result<Option<Res>, PortError> {
        match self.response_channel.receive() {
            Ok(offset) => {
                // Safety: server wrote a valid Res at this offset.
                let value = unsafe { *self.segment.resolve_ref::<Res>(offset) };
                self.segment.deallocate(offset);
                Ok(Some(value))
            }
            Err(ReceiveError::Empty) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Block until a response is received.
    pub fn receive(self) -> Result<Res, PortError> {
        loop {
            if let Some(value) = self.try_receive()? {
                return Ok(value);
            }
            std::hint::spin_loop();
        }
    }
}
