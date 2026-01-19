/// Very limited, very broken, very vibe-coded ISO-TP TX segmenter and reassembler, scheduled for removal.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IsoTpFrame {
    pub len: u8,
    pub data: [u8; 8],
}

pub struct IsoTpTx<'a> {
    data: &'a [u8],
    offset: usize,
    state: TxState,
    sn: u8, // sequence number 0..15
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TxState {
    NotStarted,
    SingleDone,
    FirstSent,
    Done,
}

impl<'a> IsoTpTx<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        IsoTpTx {
            data,
            offset: 0,
            state: TxState::NotStarted,
            sn: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsoTpPciType {
    Single,
    First,
    Consecutive,
    FlowControl,
}

impl IsoTpPciType {
    pub fn from_u8(byte0: u8) -> Option<Self> {
        match byte0 >> 4 {
            0x0 => Some(IsoTpPciType::Single),
            0x1 => Some(IsoTpPciType::First),
            0x2 => Some(IsoTpPciType::Consecutive),
            0x3 => Some(IsoTpPciType::FlowControl),
            _ => None,
        }
    }
    pub fn isotp_pci_type(bytes: [u8; 8]) -> Option<Self> {
        Self::from_u8(bytes[0])
    }
}

impl<'a> Iterator for IsoTpTx<'a> {
    type Item = IsoTpFrame;

    fn next(&mut self) -> Option<Self::Item> {
        match self.state {
            TxState::Done | TxState::SingleDone => return None,

            TxState::NotStarted => {
                let total_len = self.data.len();

                // Single Frame
                if total_len <= 7 {
                    let mut buf = [0u8; 8];
                    let pci = total_len as u8; // high nibble 0, low nibble = length
                    buf[0] = pci;
                    buf[1..1 + total_len].copy_from_slice(self.data);

                    self.state = TxState::SingleDone;

                    return Some(IsoTpFrame {
                        len: (1 + total_len) as u8,
                        data: buf,
                    });
                }

                // First Frame
                let mut buf = [0u8; 8];
                let total_len_u16 = total_len as u16;

                let hi = ((total_len_u16 >> 8) & 0x0F) as u8;
                let lo = (total_len_u16 & 0xFF) as u8;

                buf[0] = 0x10 | hi; // high nibble: 1 => First Frame
                buf[1] = lo;

                // FF carries first 6 bytes of data
                let first_chunk = 6usize.min(total_len);
                buf[2..2 + first_chunk].copy_from_slice(&self.data[..first_chunk]);
                self.offset = first_chunk;
                self.state = TxState::FirstSent;
                self.sn = 1; // first CF uses SN=1

                return Some(IsoTpFrame {
                    len: (2 + first_chunk) as u8,
                    data: buf,
                });
            }

            TxState::FirstSent => {
                if self.offset >= self.data.len() {
                    self.state = TxState::Done;
                    return None;
                }

                let mut buf = [0u8; 8];

                // CF header
                let pci = 0x20 | (self.sn & 0x0F); // high nibble 2, low nibble SN
                buf[0] = pci;

                let remaining = self.data.len() - self.offset;
                let chunk = remaining.min(7);
                buf[1..1 + chunk].copy_from_slice(&self.data[self.offset..self.offset + chunk]);

                self.offset += chunk;
                self.sn = (self.sn + 1) & 0x0F; // wrap 0..15

                if self.offset >= self.data.len() {
                    self.state = TxState::Done;
                }

                Some(IsoTpFrame {
                    len: (1 + chunk) as u8,
                    data: buf,
                })
            }
        }
    }
}

use core::cmp::min;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RxState {
    Idle,
    Receiving,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsoTpRxError {
    UnknownPciType,
    UnexpectedFrameType {
        expected: &'static str,
        got: IsoTpPciType,
    },
    LengthMismatch,
    SequenceError {
        expected: u8,
        got: u8,
    },
    Overflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsoTpRxEvent {
    None,
    FlowControlRequired,
    Completed(usize),
}

pub struct IsoTpRx {
    state: RxState,
    expected_len: Option<usize>,
    buf: [u8; 1024],
    used: usize,
    next_sn: u8, // next expected sequence number (0..15)
}

impl IsoTpRx {
    pub const fn new() -> Self {
        IsoTpRx {
            state: RxState::Idle,
            expected_len: None,
            buf: [0u8; 1024],
            used: 0,
            next_sn: 0,
        }
    }

    /// Clear current state and buffer.
    pub fn reset(&mut self) {
        self.state = RxState::Idle;
        self.expected_len = None;
        self.used = 0;
        self.next_sn = 0;
        // buffer content can stay as-is; `used` is what matters.
    }

    pub fn payload(&self) -> &[u8] {
        &self.buf[..self.used]
    }

    pub fn on_frame(&mut self, frame: &IsoTpFrame) -> Result<IsoTpRxEvent, IsoTpRxError> {
        if frame.len == 0 || frame.len as usize > 8 {
            return Err(IsoTpRxError::LengthMismatch);
        }

        let pci_type = IsoTpPciType::from_u8(frame.data[0]).ok_or(IsoTpRxError::UnknownPciType)?;

        if let IsoTpPciType::First = pci_type {}

        match pci_type {
            IsoTpPciType::Single => self.handle_single(frame),
            IsoTpPciType::First => {
                self.handle_first(frame)?;
                Ok(IsoTpRxEvent::FlowControlRequired)
            }
            IsoTpPciType::Consecutive => self.handle_consecutive(frame),
            IsoTpPciType::FlowControl => {
                // Your TX doesn’t use FC, so treat this as unexpected.
                Err(IsoTpRxError::UnexpectedFrameType {
                    expected: "Single/First/Consecutive",
                    got: IsoTpPciType::FlowControl,
                })
            }
        }
    }

    fn handle_single(&mut self, frame: &IsoTpFrame) -> Result<IsoTpRxEvent, IsoTpRxError> {
        let sf_len = (frame.data[0] & 0x0F) as usize;

        if sf_len == 0 || sf_len > 7 {
            return Err(IsoTpRxError::LengthMismatch);
        }

        if frame.len as usize != 1 + sf_len {
            return Err(IsoTpRxError::LengthMismatch);
        }

        if sf_len > self.buf.len() {
            return Err(IsoTpRxError::Overflow);
        }

        self.reset();
        self.buf[0..sf_len].copy_from_slice(&frame.data[1..1 + sf_len]);
        self.used = sf_len;
        Ok(IsoTpRxEvent::Completed(self.used))
    }

    fn handle_first(&mut self, frame: &IsoTpFrame) -> Result<Option<usize>, IsoTpRxError> {
        if frame.len < 2 {
            return Err(IsoTpRxError::LengthMismatch);
        }

        let hi = (frame.data[0] & 0x0F) as u16;
        let lo = frame.data[1] as u16;
        let total_len = ((hi << 8) | lo) as usize;

        if total_len == 0 {
            return Err(IsoTpRxError::LengthMismatch);
        }

        if total_len > self.buf.len() {
            // We don't support messages larger than our fixed buffer.
            return Err(IsoTpRxError::Overflow);
        }

        // Data starts at byte2
        let header_bytes = 2usize;
        let available = (frame.len as usize).saturating_sub(header_bytes);
        let copy_len = min(available, min(total_len, 6)); // FF can carry up to 6 data bytes.

        self.reset(); // Start a fresh multi-frame message.
        self.buf[0..copy_len].copy_from_slice(&frame.data[header_bytes..header_bytes + copy_len]);
        self.used = copy_len;
        self.expected_len = Some(total_len);
        self.state = RxState::Receiving;
        self.next_sn = 1; // next CF must have SN=1

        if self.used == total_len {
            // Slightly odd but handle gracefully.
            self.state = RxState::Idle;
            self.expected_len = None;
            return Ok(Some(self.used));
        }

        Ok(None)
    }

    fn handle_consecutive(&mut self, frame: &IsoTpFrame) -> Result<IsoTpRxEvent, IsoTpRxError> {
        if self.state != RxState::Receiving {
            return Err(IsoTpRxError::UnexpectedFrameType {
                expected: "First Frame before ConsecutiveFrame",
                got: IsoTpPciType::Consecutive,
            });
        }

        let expected_len = match self.expected_len {
            Some(l) => l,
            None => {
                return Err(IsoTpRxError::UnexpectedFrameType {
                    expected: "First Frame before ConsecutiveFrame",
                    got: IsoTpPciType::Consecutive,
                });
            }
        };

        let sn = frame.data[0] & 0x0F;
        if sn != self.next_sn {
            return Err(IsoTpRxError::SequenceError {
                expected: self.next_sn,
                got: sn,
            });
        }

        let header_bytes = 1usize;
        let payload_len = (frame.len as usize).saturating_sub(header_bytes);

        if self.used >= expected_len {
            return Err(IsoTpRxError::Overflow);
        }

        let remaining = expected_len - self.used;
        let copy_len = min(payload_len, remaining);

        // Extra safety: ensure we don't run past our fixed buffer.
        if self.used + copy_len > self.buf.len() {
            return Err(IsoTpRxError::Overflow);
        }

        self.buf[self.used..self.used + copy_len]
            .copy_from_slice(&frame.data[header_bytes..header_bytes + copy_len]);
        self.used += copy_len;

        self.next_sn = (self.next_sn + 1) & 0x0F;

        if self.used > expected_len {
            return Err(IsoTpRxError::Overflow);
        }

        if self.used == expected_len {
            // Completed payload stays in `buf[..used]`, state returns to Idle.
            self.state = RxState::Idle;
            self.expected_len = None;
            Ok(IsoTpRxEvent::Completed(self.used))
        } else {
            Ok(IsoTpRxEvent::None)
        }
    }
}

pub fn make_flow_control_cts(block_size: u8, st_min: u8) -> IsoTpFrame {
    let mut data = [0u8; 8];

    data[0] = 0x30; // PCI: FlowControl + FS=CTS
    data[1] = block_size;
    data[2] = st_min;

    IsoTpFrame { len: 3, data }
}
