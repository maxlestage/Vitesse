//! Protocol upgrades (`101 Switching Protocols`): once the response is sent,
//! the HTTP/1.1 engine hands the TCP connection over to the new protocol
//! (WebSocket).

#![cfg_attr(not(feature = "ws"), allow(dead_code))]

use std::sync::{Arc, Mutex, PoisonError};

use tokio::net::TcpStream;

use crate::handler::BoxFuture;

/// The connection the engine hands over after the `101` response.
pub(crate) struct Upgraded {
    pub(crate) io: TcpStream,
    /// Bytes already received after the request (rare: the client is
    /// supposed to wait for the `101`).
    pub(crate) read: Vec<u8>,
}

pub(crate) type OnUpgrade = Box<dyn FnOnce(Upgraded) -> BoxFuture<()> + Send>;

/// Put in the extensions of a `101` response: what to do with the
/// connection afterwards.
#[derive(Clone)]
pub(crate) struct UpgradeHandle(Arc<Mutex<Option<OnUpgrade>>>);

impl UpgradeHandle {
    pub(crate) fn new(on_upgrade: impl FnOnce(Upgraded) -> BoxFuture<()> + Send + 'static) -> Self {
        UpgradeHandle(Arc::new(Mutex::new(Some(Box::new(on_upgrade)))))
    }

    pub(crate) fn take(&self) -> Option<OnUpgrade> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).take()
    }
}
