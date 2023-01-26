use super::{router::Methods, HttpServerInner};
use crate::channel_messages::{HttpRequest, OperationRequest};
use anyhow::{anyhow, Result};
use log::{debug, error, info, warn};
use std::{collections::HashMap, io::Cursor, sync::Arc, thread};

impl TryFrom<tiny_http::Method> for crate::http_handler::Method {
    type Error = anyhow::Error;

    fn try_from(value: tiny_http::Method) -> std::result::Result<Self, Self::Error> {
        match value {
            tiny_http::Method::Get => Ok(crate::http_handler::Method::Get),
            tiny_http::Method::Post => Ok(crate::http_handler::Method::Post),
            tiny_http::Method::Put => Ok(crate::http_handler::Method::Put),
            tiny_http::Method::Delete => Ok(crate::http_handler::Method::Delete),
            tiny_http::Method::Patch => Ok(crate::http_handler::Method::Patch),
            tiny_http::Method::Head => Ok(crate::http_handler::Method::Head),
            tiny_http::Method::Options => Ok(crate::http_handler::Method::Options),
            _ => Err(anyhow!("HTTP method not supported")),
        }
    }
}

impl From<&Methods> for tiny_http::Method {
    fn from(value: &Methods) -> tiny_http::Method {
        match value {
            Methods::GET => tiny_http::Method::Get,
            Methods::PUT => tiny_http::Method::Put,
            Methods::POST => tiny_http::Method::Post,
            Methods::DELETE => tiny_http::Method::Delete,
        }
    }
}

impl From<crate::http_handler::HttpError> for tiny_http::Response<Cursor<Vec<u8>>> {
    fn from(http_error: crate::http_handler::HttpError) -> Self {
        match http_error {
            crate::http_handler::HttpError::InvalidUrl(msg) => {
                tiny_http::Response::from_data(msg.as_bytes().to_vec()).with_status_code(400)
            }
            crate::http_handler::HttpError::TimeoutError(msg) => {
                tiny_http::Response::from_data(msg.as_bytes().to_vec()).with_status_code(408)
            }
            crate::http_handler::HttpError::ProtocolError(msg) => {
                tiny_http::Response::from_data(msg.as_bytes().to_vec()).with_status_code(400)
            }
            crate::http_handler::HttpError::StatusError(code) => {
                let msg = "Unexpected error";
                tiny_http::Response::from_data(msg.as_bytes().to_vec()).with_status_code(code)
            }
            crate::http_handler::HttpError::UnexpectedError(msg) => {
                tiny_http::Response::from_data(msg.as_bytes().to_vec()).with_status_code(500)
            }
        }
    }
}

impl TryFrom<crate::http_handler::Response> for tiny_http::Response<Cursor<Vec<u8>>> {
    type Error = anyhow::Error;

    fn try_from(res: crate::http_handler::Response) -> std::result::Result<Self, Self::Error> {
        let headers = res.headers.map_or_else(
            || Ok(vec![]),
            |hdrs| {
                hdrs.iter()
                    .map(|(k, v)| {
                        tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes())
                            .map_err(|_e| anyhow!(""))
                    })
                    .collect()
            },
        )?;

        let status_code: tiny_http::StatusCode = res.status.into();
        let body: Vec<u8> = res.body.unwrap_or_default();

        Ok(tiny_http::Response::new(
            status_code,
            headers,
            Cursor::new(body.clone()),
            Some(body.len()),
            None,
        ))
    }
}

pub struct WasmHttpServer {
    inner: HttpServerInner,
    wasm_eval_tx: crossbeam_channel::Sender<OperationRequest>,
}

impl WasmHttpServer {
    pub fn new(inner: &HttpServerInner, tx: crossbeam_channel::Sender<OperationRequest>) -> Self {
        Self {
            inner: inner.to_owned(),
            wasm_eval_tx: tx,
        }
    }

    pub fn serve(&self, worker_pool_size: usize) -> Result<()> {
        let server = Arc::new(
            tiny_http::Server::http(&self.inner.address)
                .map_err(|e| anyhow!("cannot start server: {}", e))?,
        );

        let mut join_handles = Vec::with_capacity(worker_pool_size);

        for i in 0..worker_pool_size {
            let server = server.clone();
            let keep_going = self.inner.keep_going.clone();
            let wasm_eval_tx = self.wasm_eval_tx.clone();
            let routes = self.inner.router.routes.clone();

            let thread_handle = thread::spawn(move || {
                debug!("[worker #{}] building wasm handlers...", i + 1);
                let routes = match build_routes(&routes, &wasm_eval_tx) {
                    Ok(r) => r,
                    Err(e) => {
                        error!("[worker #{}] channel communication error: {:?}", i + 1, e);
                        std::process::exit(1);
                    }
                };
                debug!("[worker #{}] building wasm handlers done", i + 1);
                info!("[worker #{}] Starting http worker", i + 1);

                loop {
                    match server.try_recv() {
                        Ok(r) => {
                            if let Some(mut req) = r {
                                info!("[worker #{}] received request: {:?}", i + 1, req);
                                let routes_by_method = routes.get(req.method());
                                let response = match routes_by_method {
                                    None => {
                                        let msg = "Bad request".as_bytes().to_vec();

                                        tiny_http::Response::from_data(msg).with_status_code(400)
                                    }
                                    Some(router) => match router.recognize(req.url()) {
                                        Err(e) => {
                                            warn!(
                                                "[worker #{}] cannot find route for {} {}: {}",
                                                i + 1,
                                                req.method(),
                                                req.url(),
                                                e
                                            );
                                            let msg = "Not found".as_bytes().to_vec();
                                            tiny_http::Response::from_data(msg)
                                                .with_status_code(404)
                                        }
                                        Ok(route_match) => {
                                            info!("[worker #{}] route handler found", i + 1);
                                            process_request(&mut req, &route_match, &wasm_eval_tx)
                                        }
                                    },
                                };
                                info!("[worker #{}] sending http response", i + 1);
                                if let Err(e) = req.respond(response) {
                                    error!("Error responding to request: {}", e);
                                }
                                debug!("[worker #{}] http response sent", i + 1);
                            }
                        }
                        Err(e) => error!("error waiting for incoming request: {}", e),
                    }
                    if !*keep_going.read() {
                        info!("exiting...");
                        break;
                    }
                }
            });
            join_handles.push(thread_handle);
        }
        Ok(())
    }
}

fn build_routes(
    routes: &[crate::http_server::router::Route],
    wasm_eval_tx: &crossbeam_channel::Sender<OperationRequest>,
) -> Result<HashMap<tiny_http::Method, route_recognizer::Router<String>>> {
    let mut routes_map: HashMap<tiny_http::Method, route_recognizer::Router<String>> =
        HashMap::new();

    for route in routes {
        debug!("adding route {:?}", route);
        let method = tiny_http::Method::from(&route.method);
        let route_recognizer = match routes_map.get_mut(&method) {
            Some(r) => r,
            None => {
                let r = route_recognizer::Router::new();
                routes_map.insert(method.clone(), r);
                routes_map
                    .get_mut(&method)
                    .expect("Should not happen, the entry has just been added")
            }
        };

        let (tx, rx) = crossbeam_channel::bounded(1);
        let handler_name = route.handler.replace('_', "-");
        let register_handler_req = OperationRequest::RegisterHttpHandler {
            handler_name: handler_name.clone(),
            tx,
        };
        wasm_eval_tx.send(register_handler_req)?;

        // The first error happens if there's a communication error over
        // the channel. The second one happens if the handler is not found
        rx.recv()??;

        debug!("added route {} with handler {}", route.route, handler_name);
        route_recognizer.add(&route.route, handler_name);
    }

    Ok(routes_map)
}

fn build_http_request(
    req: &mut tiny_http::Request,
    params_iter: route_recognizer::Iter,
) -> Result<HttpRequest> {
    let mut http_req = HttpRequest::try_from(req)?;

    let params: Vec<(String, String)> = params_iter
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    http_req.params = params;

    Ok(http_req)
}

fn process_request(
    req: &mut tiny_http::Request,
    route_match: &route_recognizer::Match<&String>,
    wasm_eval_tx: &crossbeam_channel::Sender<OperationRequest>,
) -> tiny_http::Response<Cursor<Vec<u8>>> {
    let http_req = match build_http_request(req, route_match.params().iter()) {
        Ok(r) => r,
        Err(e) => {
            error!("Cannot create tiny_http Response: {:?}", e);
            let msg = "Internal server error".as_bytes().to_vec();
            return tiny_http::Response::from_data(msg).with_status_code(500);
        }
    };

    let (tx, rx) = crossbeam_channel::bounded(1);

    let handler_name: String = route_match.handler().to_string();

    let invoke_http_handler = OperationRequest::InvokeHttpHandler {
        handler_name,
        http_req,
        tx,
    };
    if let Err(e) = wasm_eval_tx.send(invoke_http_handler) {
        error!("Channel communication error: {:?}", e);
        let msg = "Internal server error".as_bytes().to_vec();
        return tiny_http::Response::from_data(msg).with_status_code(500);
    };

    let handler_response: crate::http_handler::Response = match rx.recv() {
        Err(e) => {
            error!("Channel communication error: {:?}", e);
            let msg = "Internal server error".as_bytes().to_vec();
            return tiny_http::Response::from_data(msg).with_status_code(500);
        }
        Ok(resp) => match resp {
            Err(http_error) => {
                error!("HTTP error: {:?}", http_error);
                return tiny_http::Response::from(http_error);
            }
            Ok(r) => r,
        },
    };

    tiny_http::Response::<Cursor<Vec<u8>>>::try_from(handler_response).map_or_else(
        |e| {
            error!("Cannot create tiny_http Response: {:?}", e);
            let msg = "Internal server error".as_bytes().to_vec();
            tiny_http::Response::from_data(msg).with_status_code(500)
        },
        |r| r,
    )
}
