use candive::uds::client;
use candive::uds::client::{ProtocolError, UdsClientError};

use super::TransportError;

pub struct SocketCanIsoTpSessionUdsSession {
    socket: std::cell::RefCell<socketcan_isotp::IsoTpSocket>,
}

impl SocketCanIsoTpSessionUdsSession {
    pub fn new(interface: &str, rx: u32, tx: u32) -> Result<Self, UdsClientError<TransportError>> {
        let rx_id =
            socketcan::ExtendedId::new(rx).ok_or_else(|| ProtocolError::UnexpectedResponse)?;
        let tx_id =
            socketcan::ExtendedId::new(tx).ok_or_else(|| ProtocolError::UnexpectedResponse)?;
        let socket = socketcan_isotp::IsoTpSocket::open(interface, rx_id, tx_id)
            .map_err(|_| UdsClientError::Transport(TransportError::Io))?;
        Ok(Self {
            socket: std::cell::RefCell::new(socket),
        })
    }
}

impl client::UdsTransport for SocketCanIsoTpSessionUdsSession {
    type Error = TransportError;

    fn request(&mut self, req: &[u8], resp_buf: &mut [u8]) -> Result<usize, Self::Error> {
        let mut socket = self.socket.borrow_mut();
        socket.write(req).map_err(|_| TransportError::Io)?;
        let response_slice = socket.read().map_err(|_| TransportError::Io)?;
        if response_slice.len() > resp_buf.len() {
            return Err(TransportError::Io);
        }
        resp_buf[..response_slice.len()].copy_from_slice(&response_slice);
        Ok(response_slice.len())
    }
}
