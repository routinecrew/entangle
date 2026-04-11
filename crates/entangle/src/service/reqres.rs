use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

use entangle_transport::{DataSegment, ZeroCopyChannel};

use crate::config::EntangleConfig;
use crate::contracts::{NodeId, PatternType, ReqResQos, ZeroCopySafe};
use crate::error::IpcError;
use crate::port::client::ClientBuilder;
use crate::port::server::ServerBuilder;
use crate::service::config::StaticConfig;
use crate::service::lifecycle::ServiceLifecycle;

/// Shared state connecting clients and servers within a ReqRes service.
pub(crate) struct ReqResShared {
    pub(crate) inner: Mutex<ReqResWiring>,
    pub(crate) service_name: String,
}

pub(crate) struct ReqResWiring {
    pub(crate) server: Option<ServerRegistration>,
}

pub(crate) struct ServerRegistration {
    pub(crate) node_id: NodeId,
    /// Shared segment for request and response data.
    pub(crate) segment: Arc<DataSegment>,
    /// Per-client connection info. Server checks all request channels.
    pub(crate) clients: Arc<Mutex<Vec<ClientWire>>>,
}

pub(crate) struct ClientWire {
    pub(crate) client_id: NodeId,
    /// Client -> Server: request offsets.
    pub(crate) request_channel: Arc<ZeroCopyChannel>,
    /// Server -> Client: response offsets.
    pub(crate) response_channel: Arc<ZeroCopyChannel>,
}

/// Builder for creating/opening a Request-Response service.
pub struct ReqResBuilder<Req: ZeroCopySafe, Res: ZeroCopySafe> {
    service_name: String,
    config: EntangleConfig,
    node_id: NodeId,
    qos: ReqResQos,
    _marker: PhantomData<(Req, Res)>,
}

impl<Req: ZeroCopySafe, Res: ZeroCopySafe> ReqResBuilder<Req, Res> {
    pub(crate) fn new(service_name: String, config: EntangleConfig, node_id: NodeId) -> Self {
        Self {
            service_name,
            config,
            node_id,
            qos: ReqResQos {
                max_clients: 8,
                max_servers: 1,
                max_pending_requests: 16,
            },
            _marker: PhantomData,
        }
    }

    pub fn max_clients(mut self, n: usize) -> Self {
        self.qos.max_clients = n;
        self
    }

    pub fn max_servers(mut self, n: usize) -> Self {
        self.qos.max_servers = n;
        self
    }

    pub fn max_pending_requests(mut self, n: usize) -> Self {
        self.qos.max_pending_requests = n;
        self
    }

    pub fn open_or_create(self) -> Result<ReqResService<Req, Res>, IpcError> {
        let payload_size = std::mem::size_of::<Req>().max(std::mem::size_of::<Res>());
        let payload_align = std::mem::align_of::<Req>().max(std::mem::align_of::<Res>());
        let static_config = StaticConfig::new(
            PatternType::ReqRes,
            &self.service_name,
            &format!(
                "{}:{}",
                std::any::type_name::<Req>(),
                std::any::type_name::<Res>()
            ),
            payload_size,
            payload_align,
        );

        let lifecycle = ServiceLifecycle::new(&self.config.shm_root_path());
        lifecycle.open_or_create(&static_config)?;

        let shared = Arc::new(ReqResShared {
            inner: Mutex::new(ReqResWiring { server: None }),
            service_name: self.service_name.clone(),
        });

        Ok(ReqResService {
            service_name: self.service_name,
            config: self.config,
            node_id: self.node_id,
            qos: self.qos,
            shared,
            _marker: PhantomData,
        })
    }
}

/// A Request-Response service instance.
pub struct ReqResService<Req: ZeroCopySafe, Res: ZeroCopySafe> {
    service_name: String,
    config: EntangleConfig,
    node_id: NodeId,
    qos: ReqResQos,
    pub(crate) shared: Arc<ReqResShared>,
    _marker: PhantomData<(Req, Res)>,
}

impl<Req: ZeroCopySafe, Res: ZeroCopySafe> ReqResService<Req, Res> {
    pub fn client(&self) -> ClientBuilder<Req, Res> {
        ClientBuilder::new(
            self.service_name.clone(),
            self.node_id,
            self.qos.clone(),
            self.shared.clone(),
        )
    }

    pub fn server(&self) -> ServerBuilder<Req, Res> {
        ServerBuilder::new(
            self.service_name.clone(),
            self.config.clone(),
            self.node_id,
            self.qos.clone(),
            self.shared.clone(),
        )
    }

    pub fn name(&self) -> &str {
        &self.service_name
    }
}
