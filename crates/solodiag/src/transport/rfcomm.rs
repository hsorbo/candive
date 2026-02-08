use candive::uds::client;
use candive::uds::client::UdsClientError;
use std::io::{Read, Write};
use std::time::Duration;

use super::TransportError;
use super::bt::{SlipDecoder, parse_rfcomm_datagram, rfcomm_datagram, slip_encode};

pub struct RfcommGatewayTransport {
    port: std::cell::RefCell<Box<dyn serialport::SerialPort>>,
    src: u8,
    dst: u8,
    slip_decoder: std::cell::RefCell<SlipDecoder>,
}

impl RfcommGatewayTransport {
    /// Creates a new RFCOMM gateway transport
    ///
    /// # Arguments
    /// * `port_name` - Serial port path (e.g., "/dev/rfcomm0")
    /// * `src` - Source address (local device)
    /// * `dst` - Destination address (remote device)
    pub fn new(port_name: &str, src: u8, dst: u8) -> Result<Self, UdsClientError<TransportError>> {
        let port = serialport::new(port_name, 115200)
            .timeout(Duration::from_millis(0)) // Non-blocking
            .open()
            .map_err(|_| UdsClientError::Transport(TransportError::Io))?;

        Ok(Self {
            port: std::cell::RefCell::new(port),
            src,
            dst,
            slip_decoder: std::cell::RefCell::new(SlipDecoder::new()),
        })
    }

    /// Reads a SLIP-encoded datagram from the serial port with timeout
    fn read_datagram(&self, timeout: Duration) -> Result<Vec<u8>, TransportError> {
        let mut decoder = self.slip_decoder.borrow_mut();
        let start_time = std::time::Instant::now();
        let mut read_buf = [0u8; 256];
        let mut port = self.port.borrow_mut();

        while start_time.elapsed() < timeout {
            match port.read(&mut read_buf) {
                Ok(n) if n > 0 => {
                    for byte in &read_buf[..n] {
                        if let Some(decoded_msg) = decoder.decode(*byte) {
                            return Ok(decoded_msg);
                        }
                    }
                }
                Ok(_) => {
                    // No data - short sleep to avoid busy loop
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    // Timeout is expected with non-blocking reads
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => {
                    eprintln!("✗ Serial read error: {}", e);
                    return Err(TransportError::Io);
                }
            }
        }
        eprintln!("✗ Timeout waiting for response ({:?})", timeout);
        Err(TransportError::Io)
    }
}

impl client::UdsTransport for RfcommGatewayTransport {
    type Error = TransportError;

    fn request(&mut self, req: &[u8], resp_buf: &mut [u8]) -> Result<usize, Self::Error> {
        let req_datagram = rfcomm_datagram(self.src, self.dst, req);
        let encoded = slip_encode(&req_datagram);
        {
            let mut port = self.port.borrow_mut();
            port.write_all(&encoded)?;
            port.flush()?;
        }
        let response_datagram = self.read_datagram(Duration::from_secs(5))?;

        let (resp_src, resp_dst, payload) = parse_rfcomm_datagram(&response_datagram)?;

        if resp_src != self.dst || resp_dst != self.src {
            return Err(TransportError::Io);
        }

        if payload.len() > resp_buf.len() {
            return Err(TransportError::Io);
        }

        resp_buf[..payload.len()].copy_from_slice(payload);
        Ok(payload.len())
    }
}
