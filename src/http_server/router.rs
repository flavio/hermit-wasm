// Router implementation, based on spiderlightining
// https://github.com/deislabs/spiderlightning/blob/main/crates/http-server/src/lib.rs

use super::http_server;

#[allow(clippy::upper_case_acronyms)]
#[derive(Clone, Debug, Default)]
pub enum Methods {
    #[default]
    GET,
    PUT,
    POST,
    DELETE,
}

#[derive(Clone, Debug)]
pub struct Route {
    pub method: Methods,
    pub route: String,
    pub handler: String,
}

#[derive(Clone, Debug, Default)]
pub struct RouterInner {
    /// The root directory of the filesystem
    /// This isn't used yet
    pub _base_uri: String,
    pub routes: Vec<Route>,
}

impl RouterInner {
    pub fn new(uri: &str) -> Self {
        Self {
            _base_uri: uri.to_string(),
            ..Default::default()
        }
    }

    /// Adds a new route with `GET` method and the handler's name.
    pub fn get(
        &mut self,
        route: String,
        handler: String,
    ) -> Result<Self, http_server::HttpRouterError> {
        self.add(route, handler, Methods::GET)
    }

    /// Adds a new route with `PUT` method and the handler's name.
    pub fn put(
        &mut self,
        route: String,
        handler: String,
    ) -> Result<Self, http_server::HttpRouterError> {
        self.add(route, handler, Methods::PUT)
    }

    /// Adds a new route with `POST` method and the handler's name.
    pub fn post(
        &mut self,
        route: String,
        handler: String,
    ) -> Result<Self, http_server::HttpRouterError> {
        self.add(route, handler, Methods::POST)
    }

    /// Adds a new route with `DELETE` method and the handler's name.
    pub fn delete(
        &mut self,
        route: String,
        handler: String,
    ) -> Result<Self, http_server::HttpRouterError> {
        self.add(route, handler, Methods::DELETE)
    }

    /// Adds a new route with the given method and the handler's name.
    pub fn add(
        &mut self,
        route: String,
        handler: String,
        method: Methods,
    ) -> Result<Self, http_server::HttpRouterError> {
        let route = Route {
            method,
            route,
            handler,
        };
        self.routes.push(route);
        Ok(self.clone())
    }
}
