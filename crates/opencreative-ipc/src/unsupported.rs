//! Platforms without Unix sockets: no daemon, so clients always fall back to
//! using the device directly.

use std::path::Path;

use crate::{Error, Request, Response, Result};

pub struct Client;

impl Client {
    pub fn connect(_path: &Path) -> Result<Option<Self>> {
        Ok(None)
    }

    pub fn request(&mut self, _request: &Request) -> Result<Response> {
        Err(Error::Unsupported)
    }
}

pub struct Server;

impl Server {
    pub fn bind(_path: &Path) -> Result<Self> {
        Err(Error::Unsupported)
    }

    pub fn serve<H>(&self, _handler: H) -> Result<()>
    where
        H: Fn(Request) -> Response + Send + Sync + 'static,
    {
        Err(Error::Unsupported)
    }
}
