use anyhow::{anyhow, Result};

mod bindings;

use bindings::http_handler as bindings_http_handler;
pub use bindings_http_handler::HttpHandlerData;
pub use bindings_http_handler::{HttpError, Method, Request, Response};

pub trait HttpState {
    /// Get the mutable reference to the http handler data.
    fn get_http_state_mut(&mut self) -> &mut HttpHandlerData;
}

pub fn build_http_handler<T: HttpState>(
    handler_name: &str,
    instance: &wasmi::Instance,
    store: &mut wasmi::Store<T>,
) -> Result<HttpHandler<T>> {
    HttpHandler::new(handler_name, store, instance, |ctx| {
        ctx.get_http_state_mut()
    })
}

pub struct HttpHandler<T> {
    inner: bindings_http_handler::HttpHandler<T>,
}

impl<T> HttpHandler<T> {
    pub fn new(
        handler_name: &str,
        mut store: impl wasmi::AsContextMut<UserState = T>,
        instance: &wasmi::Instance,
        get_state: impl Fn(&mut T) -> &mut HttpHandlerData + Send + Sync + Copy + 'static,
    ) -> Result<Self> {
        let mut store = store.as_context_mut();
        let canonical_abi_free =
            instance.get_typed_func::<(i32, i32, i32), ()>(&mut store, "canonical_abi_free")?;
        let canonical_abi_realloc = instance
            .get_typed_func::<(i32, i32, i32, i32), i32>(&mut store, "canonical_abi_realloc")?;
        let handle_http = instance
            .get_typed_func::<(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32), (i32,)>(
                &mut store,
                handler_name,
            )
            .map_err(|e| {
                anyhow!(
                    "Error finding exported wasm function '{}': {:?}",
                    handler_name,
                    e
                )
            })?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| anyhow::anyhow!("`memory` export not a memory"))?;
        Ok(Self {
            inner: bindings_http_handler::HttpHandler {
                canonical_abi_free,
                canonical_abi_realloc,
                handle_http,
                memory,
                get_state: Box::new(get_state),
            },
        })
    }

    pub fn handle_http(
        &self,
        caller: impl wasmi::AsContextMut<UserState = T>,
        req: Request<'_>,
    ) -> Result<Result<Response, HttpError>, wasmi::core::Trap> {
        self.inner.handle_http(caller, req)
    }
}
