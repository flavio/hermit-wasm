use crate::http_handler::{HttpHandlerData, HttpState};
use crate::http_server::{http_server, HttpServerContext, HttpServerInner};
use crate::keyvalue::{keyvalue, redis::RedisKeyvalueContext};
use crate::settings::Settings;

use anyhow::Result;
use wasmi::{AsContextMut, Linker};

pub(crate) struct HostState {
    redis_ctx: RedisKeyvalueContext,
    http_server_ctx: HttpServerContext,
    http_handler_data: HttpHandlerData,
}

impl HttpState for HostState {
    /// Get the mutable reference to the http handler data.
    fn get_http_state_mut(&mut self) -> &mut HttpHandlerData {
        &mut self.http_handler_data
    }
}

impl HostState {
    pub(crate) fn new(settings: &Settings) -> Result<Self> {
        let redis_ctx =
            RedisKeyvalueContext::new(&settings.redis_host, settings.redis_thread_pool_size)?;
        let http_server_ctx = HttpServerContext::new()?;

        Ok(Self {
            redis_ctx,
            http_server_ctx,
            http_handler_data: HttpHandlerData::default(),
        })
    }

    pub(crate) fn add_to_linker(
        linker: &mut Linker<Self>,
        store: &mut impl AsContextMut<UserState = Self>,
    ) -> Result<()> {
        keyvalue::add_to_linker(linker, store, |ctx: &mut HostState| {
            (&mut ctx.redis_ctx.kv, &mut ctx.redis_ctx.table)
        })?;

        http_server::add_to_linker(linker, store, |ctx: &mut HostState| {
            (
                &mut ctx.http_server_ctx.server,
                &mut ctx.http_server_ctx.table,
            )
        })?;
        Ok(())
    }

    pub(crate) fn server(&self) -> Option<HttpServerInner> {
        self.http_server_ctx.server.inner.clone()
    }
}
