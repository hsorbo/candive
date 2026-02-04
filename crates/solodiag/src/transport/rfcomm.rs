use candive::uds::client;
use candive::uds::client::UdsClientError;
use std::io::{Read, Write};
use std::time::Duration;

use super::TransportError;

const END: u8 = 0xC0;
const ESC: u8 = 0xDB;
const ESC_END: u8 = 0xDC;
const ESC_ESC: u8 = 0xDD;

/// SLIP encoder - encodes data with SLIP framing
fn slip_encode(data: &[u8]) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(data.len() + 2);

    for &byte in data {
        match byte {
            END => {
                encoded.push(ESC);
                encoded.push(ESC_END);
            }
            ESC => {
                encoded.push(ESC);
                encoded.push(ESC_ESC);
            }
            _ => encoded.push(byte),
        }
    }

    encoded.push(END);
    encoded
}

/// SLIP decoder - stateful decoder for processing bytes one at a time
struct SlipDecoder {
    buffer: Vec<u8>,
    escape: bool,
}

impl SlipDecoder {
    fn new() -> Self {
        SlipDecoder {
            buffer: Vec::new(),
            escape: false,
        }
    }

    fn decode(&mut self, byte: u8) -> Option<Vec<u8>> {
        match byte {
            END => {
                if !self.buffer.is_empty() {
                    let msg = self.buffer.clone();
                    self.buffer.clear();
                    self.escape = false;
                    return Some(msg);
                }
            }
            ESC => {
                self.escape = true;
            }
            _ => {
                if self.escape {
                    match byte {
                        ESC_END => self.buffer.push(END),
                        ESC_ESC => self.buffer.push(ESC),
                        _ => self.buffer.push(byte),
                    }
                    self.escape = false;
                } else {
                    self.buffer.push(byte);
                }
            }
        }
        None
    }
}

fn datagram(src: u8, dst: u8, data: &[u8]) -> Vec<u8> {
    let data_length = data.len();

    if data_length > 0xFF {
        panic!("Data too long for 1-byte length field");
    }

    let mut result = Vec::with_capacity(3 + data_length);
    result.push(src);
    result.push(dst);
    result.push(data_length as u8);
    result.extend_from_slice(data);
    result
}

fn parse_datagram(data: &[u8]) -> Result<(u8, u8, &[u8]), TransportError> {
    if data.len() < 3 {
        return Err(TransportError::Io);
    }

    let src = data[0];
    let dst = data[1];
    let len = data[2] as usize;

    if data.len() < 3 + len {
        return Err(TransportError::Io);
    }

    Ok((src, dst, &data[3..3 + len]))
}

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
        let req_datagram = datagram(self.src, self.dst, req);
        let encoded = slip_encode(&req_datagram);
        {
            let mut port = self.port.borrow_mut();
            port.write_all(&encoded)?;
            port.flush()?;
        }
        let response_datagram = self.read_datagram(Duration::from_secs(5))?;

        let (resp_src, resp_dst, payload) = parse_datagram(&response_datagram)?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_datagram_rdbi_response() {
        // Real RDBI response (after SLIP decoding): Read DID 0x8011 -> "V72 DiveCAN"
        // Frame: [src=0x80][dst=0xff][len=0x0f][UDS payload: 62 80 11 56 37 32 20 44 69 76 65 43 41 4e]
        let raw = hex::decode("80ff0f00628011563732204469766543414e").unwrap();

        let (src, dst, payload) = parse_datagram(&raw).expect("parse should succeed");

        assert_eq!(src, 0x80, "source should be 0x80");
        assert_eq!(dst, 0xff, "destination should be 0xff");
        assert_eq!(payload.len(), 15, "payload length should be 15");

        assert_eq!(payload[0], 0x00, "first byte should be 0x00");
        assert_eq!(
            payload[1], 0x62,
            "service should be 0x62 (positive RDBI response)"
        );
        assert_eq!(payload[2], 0x80, "DID high byte");
        assert_eq!(payload[3], 0x11, "DID low byte");

        // Verify the ASCII data portion: "V72 DiveCAN"
        let data_str = std::str::from_utf8(&payload[4..]).expect("should be valid ASCII");
        assert_eq!(data_str, "V72 DiveCAN");
    }
}
