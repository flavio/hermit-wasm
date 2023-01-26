wit_bindgen_wasmi::export!({paths: ["wit/http-server.wit"]});

mod router;
mod server;

use crate::host_state::HostState;
use crate::http_handler::{build_http_handler, HttpHandler};
use crate::{channel_messages::OperationRequest, settings::Settings};

use anyhow::{anyhow, Result};
use http_server::{HttpRouterError, HttpServerTables, Uri};
use log::{debug, error, info, warn};
use parking_lot::RwLock;
use router::RouterInner;
use server::WasmHttpServer;
use std::{collections::HashMap, sync::Arc};

#[derive(Debug, Clone)]
pub struct HttpServerInner {
    pub router: RouterInner,
    pub address: String,
    pub keep_going: Arc<RwLock<bool>>,
}

impl HttpServerInner {
    pub fn new(address: &str, router: &RouterInner) -> Self {
        Self {
            router: router.to_owned(),
            address: address.to_owned(),
            keep_going: Arc::new(RwLock::new(true)),
        }
    }

    pub fn stop(&mut self) -> Result<()> {
        info!("requesting http server to stop");
        let mut keep_going = self.keep_going.write();
        *keep_going = false;

        Ok(())
    }
}

#[derive(Default)]
pub struct HttpServerImplementor {
    pub inner: Option<HttpServerInner>,
}

impl http_server::HttpServer for HttpServerImplementor {
    type Router = RouterInner;
    type Server = HttpServerInner;

    /// create a new HTTP router
    fn router_new(&mut self) -> Result<Self::Router, HttpRouterError> {
        Ok(RouterInner::default())
    }

    /// create a new HTTP router
    fn router_new_with_base(&mut self, base: Uri<'_>) -> Result<Self::Router, HttpRouterError> {
        Ok(RouterInner::new(base))
    }

    /// register a HTTP GET route
    fn router_get(
        &mut self,
        router: &Self::Router,
        route: &str,
        handler: &str,
    ) -> Result<Self::Router, HttpRouterError> {
        // Router is a reference to the router proxy, so we need to clone it to get a
        // mutable reference to the router.
        let mut rclone = router.clone();
        rclone.get(route.to_string(), handler.to_string())
    }

    /// register a HTTP PUT route
    fn router_put(
        &mut self,
        router: &Self::Router,
        route: &str,
        handler: &str,
    ) -> Result<Self::Router, HttpRouterError> {
        // Router is a reference to the router proxy, so we need to clone it to get a
        // mutable reference to the router.
        let mut rclone = router.clone();
        rclone.put(route.to_string(), handler.to_string())
    }

    /// register a HTTP POST route
    fn router_post(
        &mut self,
        router: &Self::Router,
        route: &str,
        handler: &str,
    ) -> Result<Self::Router, HttpRouterError> {
        // Router is a reference to the router proxy, so we need to clone it to get a
        // mutable reference to the router.
        let mut rclone = router.clone();
        rclone.post(route.to_string(), handler.to_string())
    }

    /// register a HTTP DELETE route
    fn router_delete(
        &mut self,
        router: &Self::Router,
        route: &str,
        handler: &str,
    ) -> Result<Self::Router, HttpRouterError> {
        // Router is a reference to the router proxy, so we need to clone it to get a
        // mutable reference to the router.
        let mut rclone = router.clone();
        rclone.delete(route.to_string(), handler.to_string())
    }

    /// create a new HTTP server and serve the given router
    fn server_serve(
        &mut self,
        address: &str,
        router: &Self::Router,
    ) -> Result<Self::Server, HttpRouterError> {
        debug!("server_serve - address {} - router {:?}", address, router);
        let server = HttpServerInner::new(address, router);
        self.inner = Some(server.clone());

        Ok(server)
    }

    /// stop the server
    fn server_stop(&mut self, server: &Self::Server) -> Result<(), HttpRouterError> {
        // Server is a reference to the server proxy, so we need to clone it to get a
        // mutable reference to the server.
        let mut sclone = server.clone();
        sclone
            .stop()
            .map_err(|e| HttpRouterError::UnexpectedError(e.to_string()))
    }
}

pub struct HttpServerContext {
    pub server: HttpServerImplementor,
    pub table: HttpServerTables<HttpServerImplementor>,
}

impl HttpServerContext {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            server: HttpServerImplementor::default(),
            table: HttpServerTables::<HttpServerImplementor>::default(),
        })
    }
}

pub(crate) fn start_http_server_loop(
    http_inner_server: &HttpServerInner,
    settings: &Settings,
    instance: &wasmi::Instance,
    store: &mut wasmi::Store<HostState>,
) -> Result<()> {
    let mut http_handlers: HashMap<String, HttpHandler<HostState>> = HashMap::new();

    let (tx, rx) = crossbeam_channel::bounded::<OperationRequest>(100);

    let server = WasmHttpServer::new(http_inner_server, tx);
    server.serve(settings.http_server_worker_pool_size)?;

    loop {
        match rx.recv() {
            Ok(req) => {
                debug!("Got something to do: {:?}", req);
                match req {
                    OperationRequest::RegisterHttpHandler { handler_name, tx } => {
                        debug!("registering http handler with name: '{}'", handler_name);

                        let res = if let std::collections::hash_map::Entry::Vacant(e) =
                            http_handlers.entry(handler_name.clone())
                        {
                            debug!(
                                "looking for '{}' handler inside of wasm module",
                                handler_name
                            );

                            match build_http_handler(&handler_name, instance, &mut *store) {
                                Ok(h) => {
                                    e.insert(h);
                                    Ok(())
                                }
                                Err(e) => Err(anyhow!("Cannot find handler: {}", e)),
                            }
                        } else {
                            debug!("'{}' handler is already known", handler_name);
                            Ok(())
                        };
                        if let Err(e) = tx.try_send(res) {
                            error!("channel communication error: {}", e);
                        };
                    }
                    OperationRequest::InvokeHttpHandler {
                        handler_name,
                        http_req,
                        tx,
                    } => {
                        let res = match http_handlers.get(&handler_name) {
                            None => {
                                warn!("Cannot find handler with name: {}", handler_name);
                                Err(crate::http_handler::HttpError::StatusError(400))
                            }
                            Some(handler) => {
                                debug!("invoking http handler '{}'", handler_name);
                                let mut headers: Vec<(&str, &str)> = vec![];
                                for (k, v) in &http_req.headers {
                                    headers.push((k.as_str(), v.as_str()));
                                }

                                let mut params: Vec<(&str, &str)> = vec![];
                                for (k, v) in &http_req.params {
                                    params.push((k.as_str(), v.as_str()));
                                }

                                let body = http_req.body.as_deref();

                                let handler_req = crate::http_handler::Request {
                                    method: http_req.method,
                                    uri: &http_req.uri,
                                    headers: &headers,
                                    params: &params,
                                    body,
                                };

                                match handler.handle_http(&mut *store, handler_req) {
                                    Err(e) => {
                                        error!("http handler wasm error: {}", e);
                                        Err(crate::http_handler::HttpError::StatusError(500))
                                    }
                                    Ok(r) => {
                                        debug!("'{}' provided response", handler_name);
                                        r
                                    }
                                }
                            }
                        };
                        if let Err(e) = tx.try_send(res) {
                            error!("channel communication error: {}", e);
                        };
                    }
                }
            }
            Err(e) => {
                error!("Error trying to receive message from channel: {}", e);
            }
        }
    }
}
