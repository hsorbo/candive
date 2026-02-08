use super::TransportError;

const END: u8 = 0xC0;
const ESC: u8 = 0xDB;
const ESC_END: u8 = 0xDC;
const ESC_ESC: u8 = 0xDD;

/// SLIP encoder - encodes data with SLIP framing
pub fn slip_encode(data: &[u8]) -> Vec<u8> {
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
pub struct SlipDecoder {
    buffer: Vec<u8>,
    escape: bool,
}

impl SlipDecoder {
    pub fn new() -> Self {
        SlipDecoder {
            buffer: Vec::new(),
            escape: false,
        }
    }

    pub fn decode(&mut self, byte: u8) -> Option<Vec<u8>> {
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

/// Creates a datagram with source, destination, length, and payload
pub fn rfcomm_datagram(src: u8, dst: u8, data: &[u8]) -> Vec<u8> {
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

pub fn parse_rfcomm_datagram(data: &[u8]) -> Result<(u8, u8, &[u8]), TransportError> {
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

pub fn ble_datagram(src: u8, dst: u8, data: &[u8]) -> Vec<u8> {
    let data_length = data.len();

    if data_length > 0xFF {
        panic!("Data too long for 1-byte length field");
    }

    let mut result = Vec::with_capacity(3 + data_length);

    result.push(0x01);
    result.push(0x00);
    result.push(src);
    result.push(dst);
    result.push(data_length as u8);
    result.extend_from_slice(data);
    result
}

pub fn parse_ble_datagram(data: &[u8]) -> Result<(u8, u8, &[u8]), TransportError> {
    parse_rfcomm_datagram(&data[2..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_datagram_rdbi_response() {
        // Real RDBI response (after SLIP decoding): Read DID 0x8011 -> "V72 DiveCAN"
        // Frame: [src=0x80][dst=0xff][len=0x0f][UDS payload: 62 80 11 56 37 32 20 44 69 76 65 43 41 4e]
        let raw = hex::decode("80ff0f00628011563732204469766543414e").unwrap();

        let (src, dst, payload) = parse_rfcomm_datagram(&raw).expect("parse should succeed");

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
