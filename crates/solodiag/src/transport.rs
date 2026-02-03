use candive::uds::client;
use candive::uds::client::ProtocolError;
use candive::uds::client::UdsClientError;
use candive::uds::isotp::IsoTpRxError;

/// Transport-specific error type for solodiag
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    /// ISO-TP protocol error
    IsoTp(IsoTpRxError),
    /// I/O error
    Io,
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::IsoTp(e) => write!(f, "ISO-TP error: {:?}", e),
            TransportError::Io => write!(f, "I/O error"),
        }
    }
}

impl std::error::Error for TransportError {}

impl From<IsoTpRxError> for TransportError {
    fn from(e: IsoTpRxError) -> Self {
        TransportError::IsoTp(e)
    }
}

impl From<std::io::Error> for TransportError {
    fn from(_: std::io::Error) -> Self {
        TransportError::Io
    }
}

pub fn uds_error_to_anyhow(
    err: candive::uds::client::UdsClientError<TransportError>,
) -> anyhow::Error {
    use candive::uds::client::UdsClientError;

    match err {
        UdsClientError::Transport(e) => anyhow::anyhow!("Transport error: {}", e),
        UdsClientError::NegativeResponse(neg) => anyhow::anyhow!(
            "Negative response: service=0x{:02X}, code=0x{:02X}",
            neg.service,
            neg.code.as_u8()
        ),
        UdsClientError::Decode(e) => anyhow::anyhow!("Decode error: {:?}", e),
        UdsClientError::Encode(e) => anyhow::anyhow!("Encode error: {:?}", e),
        UdsClientError::Protocol(e) => anyhow::anyhow!("Protocol error: {:?}", e),
        UdsClientError::ResponseTooLarge => anyhow::anyhow!("Response too large for buffer"),
    }
}

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
