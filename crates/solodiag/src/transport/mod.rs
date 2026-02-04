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

// Linux-only SocketCAN transport
#[cfg(target_os = "linux")]
mod socketcan;
#[cfg(target_os = "linux")]
pub use socketcan::SocketCanIsoTpSessionUdsSession;

// Cross-platform RFCOMM transport
mod rfcomm;
pub use rfcomm::RfcommGatewayTransport;
