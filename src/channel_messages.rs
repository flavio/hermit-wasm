use anyhow::Result;
use crossbeam_channel::Sender;

#[derive(Debug)]
pub struct HttpRequest {
    pub method: crate::http_handler::Method,
    pub uri: String,
    pub headers: Vec<(String, String)>,
    pub params: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

impl TryFrom<&mut tiny_http::Request> for HttpRequest {
    type Error = anyhow::Error;

    fn try_from(req: &mut tiny_http::Request) -> std::result::Result<Self, Self::Error> {
        let method = crate::http_handler::Method::try_from(req.method().to_owned())?;

        let headers: Vec<(String, String)> = req
            .headers()
            .to_owned()
            .iter()
            .map(|header| {
                (
                    header.field.as_str().as_str().to_string(),
                    header.value.as_str().to_string(),
                )
            })
            .collect();

        // These cannot be added now
        let params: Vec<(String, String)> = vec![];

        let mut body: Vec<u8> = Vec::with_capacity(req.body_length().unwrap_or_default());
        req.as_reader().read_to_end(&mut body)?;

        Ok(Self {
            uri: req.url().to_string(),
            method,
            headers,
            params,
            body: Some(body),
        })
    }
}

#[derive(Debug)]
pub enum OperationRequest {
    InvokeHttpHandler {
        handler_name: String,
        http_req: HttpRequest,
        tx: Sender<
            std::result::Result<crate::http_handler::Response, crate::http_handler::HttpError>,
        >,
    },
    RegisterHttpHandler {
        handler_name: String,
        tx: Sender<Result<()>>,
    },
}
