#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UdsDecodeError {
    TooShort { needed: usize },
    BadLength { expected: usize },
    InvalidFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UdsEncodeError {
    BufferTooSmall { needed: usize, capacity: usize },
}

pub const DFI_PLAIN: u8 = 0x00;
pub const ALFI_ADDR4_SIZE4: u8 = (4 << 4) | 4;

pub const SID_RDBI_REQ: u8 = 0x22;
pub const SID_RDBI_RESP: u8 = 0x62;

pub const SID_WDBI_REQ: u8 = 0x2e;
pub const SID_WDBI_RESP: u8 = 0x6e;

pub const SID_REQUEST_DOWNLOAD_REQ: u8 = 0x34;
pub const SID_REQUEST_DOWNLOAD_RESP: u8 = 0x74;

pub const SID_REQUEST_UPLOAD_REQ: u8 = 0x35;
pub const SID_REQUEST_UPLOAD_RESP: u8 = 0x75;

pub const SID_TRANSFER_DATA_REQ: u8 = 0x36;
pub const SID_TRANSFER_DATA_RESP: u8 = 0x76;

pub const SID_TRANSFER_EXIT_REQ: u8 = 0x37;
pub const SID_TRANSFER_EXIT_RESP: u8 = 0x77;

pub const SID_NEG_RESPONSE: u8 = 0x7F;

pub const DIVE_CAN_UDS_ADDR: u8 = 0x00;

/// Read-only view over a UDS PDU (received message)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UdsPduView<'a> {
    bytes: &'a [u8],
}

impl<'a> UdsPduView<'a> {
    /// Create a new view from bytes
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    /// Get the raw bytes
    pub fn as_bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// Ensure minimum length
    fn ensure_len(&self, needed: usize) -> Result<(), UdsDecodeError> {
        if self.bytes.len() < needed {
            Err(UdsDecodeError::TooShort { needed })
        } else {
            Ok(())
        }
    }

    /// Get the service ID (SID) - byte at index 1
    pub fn sid(&self) -> Result<u8, UdsDecodeError> {
        self.ensure_len(2)?;
        Ok(self.bytes[1])
    }

    /// Check if this is a negative response and return error if so
    pub fn check_positive(&self) -> Result<(), NegativeResponse> {
        if self.bytes.len() >= 4 && self.bytes[1] == SID_NEG_RESPONSE {
            Err(NegativeResponse {
                service: self.bytes[2],
                code: UdsErrorCode::from_u8(self.bytes[3]),
            })
        } else {
            Ok(())
        }
    }

    /// Ensure minimum length and verify expected SID
    pub fn expect_sid(&self, sid: u8, min_len: usize) -> Result<(), UdsDecodeError> {
        self.ensure_len(min_len)?;
        if self.bytes[1] != sid {
            return Err(UdsDecodeError::InvalidFormat);
        }
        Ok(())
    }
}

/// Writer for building UDS PDUs into a caller-provided buffer
#[derive(Debug)]
pub struct UdsPduWriter<'a> {
    buf: &'a mut [u8],
    len: usize,
}

impl<'a> UdsPduWriter<'a> {
    /// Create a new writer with the given buffer
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, len: 0 }
    }

    /// Get the current content as bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }

    /// Get the current length
    pub fn len(&self) -> usize {
        self.len
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Set the UDS header (address + SID)
    pub fn set_header(&mut self, sid: u8) -> Result<(), UdsEncodeError> {
        if self.buf.len() < 2 {
            return Err(UdsEncodeError::BufferTooSmall {
                needed: 2,
                capacity: self.buf.len(),
            });
        }
        self.buf[0] = DIVE_CAN_UDS_ADDR;
        self.buf[1] = sid;
        self.len = 2;
        Ok(())
    }

    /// Push payload bytes
    pub fn push(&mut self, payload: &[u8]) -> Result<(), UdsEncodeError> {
        let needed = self.len + payload.len();
        if needed > self.buf.len() {
            return Err(UdsEncodeError::BufferTooSmall {
                needed,
                capacity: self.buf.len(),
            });
        }
        self.buf[self.len..self.len + payload.len()].copy_from_slice(payload);
        self.len += payload.len();
        Ok(())
    }

    /// Build a negative response directly into this writer
    pub fn make_negative_response(
        buf: &'a mut [u8],
        service: u8,
        code: UdsErrorCode,
    ) -> Result<Self, UdsEncodeError> {
        if buf.len() < 4 {
            return Err(UdsEncodeError::BufferTooSmall {
                needed: 4,
                capacity: buf.len(),
            });
        }
        buf[0] = DIVE_CAN_UDS_ADDR;
        buf[1] = SID_NEG_RESPONSE;
        buf[2] = service;
        buf[3] = code.as_u8();
        Ok(Self { buf, len: 4 })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NegativeResponse {
    pub service: u8,
    pub code: UdsErrorCode,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UdsErrorCode {
    GeneralReject,
    IncorrectMessageLengthOrInvalidFormat,
    BusyRepeatRequest,
    ConditionsNotCorrect,
    RequestSequenceError,
    RequestOutOfRange,
    AuthenticationRequired,
    GeneralProgrammingFailure,
    WrongBlockSequenceCounter,
    Unknown(u8),
}

impl UdsErrorCode {
    pub fn from_u8(value: u8) -> Self {
        match value {
            0x10 => UdsErrorCode::GeneralReject,
            0x13 => UdsErrorCode::IncorrectMessageLengthOrInvalidFormat,
            0x21 => UdsErrorCode::BusyRepeatRequest,
            0x22 => UdsErrorCode::ConditionsNotCorrect,
            0x24 => UdsErrorCode::RequestSequenceError,
            0x31 => UdsErrorCode::RequestOutOfRange,
            0x34 => UdsErrorCode::AuthenticationRequired,
            0x72 => UdsErrorCode::GeneralProgrammingFailure,
            0x73 => UdsErrorCode::WrongBlockSequenceCounter,
            other => UdsErrorCode::Unknown(other),
        }
    }

    pub fn as_u8(self) -> u8 {
        match self {
            UdsErrorCode::GeneralReject => 0x10,
            UdsErrorCode::IncorrectMessageLengthOrInvalidFormat => 0x13,
            UdsErrorCode::BusyRepeatRequest => 0x21,
            UdsErrorCode::ConditionsNotCorrect => 0x22,
            UdsErrorCode::RequestSequenceError => 0x24,
            UdsErrorCode::RequestOutOfRange => 0x31,
            UdsErrorCode::AuthenticationRequired => 0x34,
            UdsErrorCode::GeneralProgrammingFailure => 0x72,
            UdsErrorCode::WrongBlockSequenceCounter => 0x73,
            UdsErrorCode::Unknown(other) => other,
        }
    }
}

pub trait ServiceCodec {
    type Request<'a>;
    type Response<'a>;
    const REQ_SID: u8;
    const RESP_SID: u8;

    fn encode_request(
        req: &Self::Request<'_>,
        out: &mut UdsPduWriter<'_>,
    ) -> Result<(), UdsEncodeError>;
    fn decode_request<'a>(pdu: UdsPduView<'a>) -> Result<Self::Request<'a>, UdsDecodeError>;

    fn encode_response(
        resp: &Self::Response<'_>,
        out: &mut UdsPduWriter<'_>,
    ) -> Result<(), UdsEncodeError>;
    fn decode_response<'a>(pdu: UdsPduView<'a>) -> Result<Self::Response<'a>, UdsDecodeError>;
}

// ReadByIdentifier (0x22 / 0x62)
pub struct ReadByIdentifierCodec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadByIdentifierReq {
    pub did: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadByIdentifierResp<'a> {
    pub did: u16,
    pub data: &'a [u8],
}

impl ServiceCodec for ReadByIdentifierCodec {
    type Request<'a> = ReadByIdentifierReq;
    type Response<'a> = ReadByIdentifierResp<'a>;

    const REQ_SID: u8 = SID_RDBI_REQ;
    const RESP_SID: u8 = SID_RDBI_RESP;

    fn encode_request(
        req: &Self::Request<'_>,
        out: &mut UdsPduWriter<'_>,
    ) -> Result<(), UdsEncodeError> {
        out.set_header(Self::REQ_SID)?;
        out.push(&req.did.to_be_bytes())?;
        Ok(())
    }

    fn decode_request<'a>(pdu: UdsPduView<'a>) -> Result<Self::Request<'a>, UdsDecodeError> {
        pdu.expect_sid(Self::REQ_SID, 4)?;
        let did = u16::from_be_bytes([pdu.as_bytes()[2], pdu.as_bytes()[3]]);
        Ok(ReadByIdentifierReq { did })
    }

    fn encode_response(
        resp: &Self::Response<'_>,
        out: &mut UdsPduWriter<'_>,
    ) -> Result<(), UdsEncodeError> {
        out.set_header(Self::RESP_SID)?;
        out.push(&resp.did.to_be_bytes())?;
        out.push(resp.data)?;
        Ok(())
    }

    fn decode_response<'a>(pdu: UdsPduView<'a>) -> Result<Self::Response<'a>, UdsDecodeError> {
        pdu.expect_sid(Self::RESP_SID, 4)?;
        let did = u16::from_be_bytes([pdu.as_bytes()[2], pdu.as_bytes()[3]]);
        let data = &pdu.as_bytes()[4..];
        Ok(ReadByIdentifierResp { did, data })
    }
}

// WriteByIdentifier (0x2E / 0x6E)
pub struct WriteByIdentifierCodec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteByIdentifierReq<'a> {
    pub did: u16,
    pub data: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteByIdentifierResp {
    pub did: u16,
}

impl ServiceCodec for WriteByIdentifierCodec {
    type Request<'a> = WriteByIdentifierReq<'a>;
    type Response<'a> = WriteByIdentifierResp;

    const REQ_SID: u8 = SID_WDBI_REQ;
    const RESP_SID: u8 = SID_WDBI_RESP;

    fn encode_request(
        req: &Self::Request<'_>,
        out: &mut UdsPduWriter<'_>,
    ) -> Result<(), UdsEncodeError> {
        out.set_header(Self::REQ_SID)?;
        out.push(&req.did.to_be_bytes())?;
        out.push(req.data)?;
        Ok(())
    }

    fn decode_request<'a>(pdu: UdsPduView<'a>) -> Result<Self::Request<'a>, UdsDecodeError> {
        pdu.expect_sid(Self::REQ_SID, 4)?;
        let did = u16::from_be_bytes([pdu.as_bytes()[2], pdu.as_bytes()[3]]);
        let data = &pdu.as_bytes()[4..];
        Ok(WriteByIdentifierReq { did, data })
    }

    fn encode_response(
        resp: &Self::Response<'_>,
        out: &mut UdsPduWriter<'_>,
    ) -> Result<(), UdsEncodeError> {
        out.set_header(Self::RESP_SID)?;
        out.push(&resp.did.to_be_bytes())?;
        Ok(())
    }

    fn decode_response<'a>(pdu: UdsPduView<'a>) -> Result<Self::Response<'a>, UdsDecodeError> {
        pdu.expect_sid(Self::RESP_SID, 4)?;
        let did = u16::from_be_bytes([pdu.as_bytes()[2], pdu.as_bytes()[3]]);
        Ok(WriteByIdentifierResp { did })
    }
}

// RequestDownload (0x34 / 0x74)
pub struct RequestDownloadCodec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestDownloadReq {
    pub address: u32,
    pub size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestDownloadResp<'a> {
    pub payload: &'a [u8],
}

impl ServiceCodec for RequestDownloadCodec {
    type Request<'a> = RequestDownloadReq;
    type Response<'a> = RequestDownloadResp<'a>;

    const REQ_SID: u8 = SID_REQUEST_DOWNLOAD_REQ;
    const RESP_SID: u8 = SID_REQUEST_DOWNLOAD_RESP;

    fn encode_request(
        req: &Self::Request<'_>,
        out: &mut UdsPduWriter<'_>,
    ) -> Result<(), UdsEncodeError> {
        out.set_header(Self::REQ_SID)?;
        out.push(&[DFI_PLAIN, ALFI_ADDR4_SIZE4])?;
        out.push(&req.address.to_be_bytes())?;
        out.push(&req.size.to_be_bytes())?;
        Ok(())
    }

    fn decode_request<'a>(pdu: UdsPduView<'a>) -> Result<Self::Request<'a>, UdsDecodeError> {
        pdu.expect_sid(Self::REQ_SID, 12)?;
        let bytes = pdu.as_bytes();
        if bytes[2] != DFI_PLAIN || bytes[3] != ALFI_ADDR4_SIZE4 {
            return Err(UdsDecodeError::InvalidFormat);
        }
        let address = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let size = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        Ok(RequestDownloadReq { address, size })
    }

    fn encode_response(
        resp: &Self::Response<'_>,
        out: &mut UdsPduWriter<'_>,
    ) -> Result<(), UdsEncodeError> {
        out.set_header(Self::RESP_SID)?;
        out.push(resp.payload)?;
        Ok(())
    }

    fn decode_response<'a>(pdu: UdsPduView<'a>) -> Result<Self::Response<'a>, UdsDecodeError> {
        pdu.expect_sid(Self::RESP_SID, 2)?;
        let payload = &pdu.as_bytes()[2..];
        Ok(RequestDownloadResp { payload })
    }
}

// RequestUpload (0x35 / 0x75)
pub struct RequestUploadCodec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestUploadReq {
    pub address: u32,
    pub size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestUploadResp<'a> {
    pub payload: &'a [u8],
}

impl ServiceCodec for RequestUploadCodec {
    type Request<'a> = RequestUploadReq;
    type Response<'a> = RequestUploadResp<'a>;

    const REQ_SID: u8 = SID_REQUEST_UPLOAD_REQ;
    const RESP_SID: u8 = SID_REQUEST_UPLOAD_RESP;

    fn encode_request(
        req: &Self::Request<'_>,
        out: &mut UdsPduWriter<'_>,
    ) -> Result<(), UdsEncodeError> {
        out.set_header(Self::REQ_SID)?;
        out.push(&[DFI_PLAIN, ALFI_ADDR4_SIZE4])?;
        out.push(&req.address.to_be_bytes())?;
        out.push(&req.size.to_be_bytes())?;
        Ok(())
    }

    fn decode_request<'a>(pdu: UdsPduView<'a>) -> Result<Self::Request<'a>, UdsDecodeError> {
        pdu.expect_sid(Self::REQ_SID, 12)?;
        let bytes = pdu.as_bytes();
        if bytes[2] != DFI_PLAIN || bytes[3] != ALFI_ADDR4_SIZE4 {
            return Err(UdsDecodeError::InvalidFormat);
        }
        let address = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let size = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        Ok(RequestUploadReq { address, size })
    }

    fn encode_response(
        resp: &Self::Response<'_>,
        out: &mut UdsPduWriter<'_>,
    ) -> Result<(), UdsEncodeError> {
        out.set_header(Self::RESP_SID)?;
        out.push(resp.payload)?;
        Ok(())
    }

    fn decode_response<'a>(pdu: UdsPduView<'a>) -> Result<Self::Response<'a>, UdsDecodeError> {
        pdu.expect_sid(Self::RESP_SID, 2)?;
        let payload = &pdu.as_bytes()[2..];
        Ok(RequestUploadResp { payload })
    }
}

// TransferData (0x36 / 0x76)
pub struct TransferDataCodec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferDataReq<'a> {
    pub block_seq: u8,
    pub payload: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferDataResp<'a> {
    pub block_seq: u8,
    pub payload: &'a [u8],
}

impl ServiceCodec for TransferDataCodec {
    type Request<'a> = TransferDataReq<'a>;
    type Response<'a> = TransferDataResp<'a>;

    const REQ_SID: u8 = SID_TRANSFER_DATA_REQ;
    const RESP_SID: u8 = SID_TRANSFER_DATA_RESP;

    fn encode_request(
        req: &Self::Request<'_>,
        out: &mut UdsPduWriter<'_>,
    ) -> Result<(), UdsEncodeError> {
        out.set_header(Self::REQ_SID)?;
        out.push(&[req.block_seq])?;
        out.push(req.payload)?;
        Ok(())
    }

    fn decode_request<'a>(pdu: UdsPduView<'a>) -> Result<Self::Request<'a>, UdsDecodeError> {
        pdu.expect_sid(Self::REQ_SID, 3)?;
        let block_seq = pdu.as_bytes()[2];
        let payload = &pdu.as_bytes()[3..];
        Ok(TransferDataReq { block_seq, payload })
    }

    fn encode_response(
        resp: &Self::Response<'_>,
        out: &mut UdsPduWriter<'_>,
    ) -> Result<(), UdsEncodeError> {
        out.set_header(Self::RESP_SID)?;
        out.push(&[resp.block_seq])?;
        out.push(resp.payload)?;
        Ok(())
    }

    fn decode_response<'a>(pdu: UdsPduView<'a>) -> Result<Self::Response<'a>, UdsDecodeError> {
        pdu.expect_sid(Self::RESP_SID, 3)?;
        let block_seq = pdu.as_bytes()[2];
        let payload = &pdu.as_bytes()[3..];
        Ok(TransferDataResp { block_seq, payload })
    }
}

// TransferExit (0x37 / 0x77)
pub struct TransferExitCodec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferExitReq;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferExitResp;

impl ServiceCodec for TransferExitCodec {
    type Request<'a> = TransferExitReq;
    type Response<'a> = TransferExitResp;

    const REQ_SID: u8 = SID_TRANSFER_EXIT_REQ;
    const RESP_SID: u8 = SID_TRANSFER_EXIT_RESP;

    fn encode_request(
        _: &Self::Request<'_>,
        out: &mut UdsPduWriter<'_>,
    ) -> Result<(), UdsEncodeError> {
        out.set_header(Self::REQ_SID)?;
        Ok(())
    }

    fn decode_request<'a>(pdu: UdsPduView<'a>) -> Result<Self::Request<'a>, UdsDecodeError> {
        pdu.expect_sid(Self::REQ_SID, 2)?;
        Ok(TransferExitReq)
    }

    fn encode_response(
        _: &Self::Response<'_>,
        out: &mut UdsPduWriter<'_>,
    ) -> Result<(), UdsEncodeError> {
        out.set_header(Self::RESP_SID)?;
        Ok(())
    }

    fn decode_response<'a>(pdu: UdsPduView<'a>) -> Result<Self::Response<'a>, UdsDecodeError> {
        pdu.expect_sid(Self::RESP_SID, 2)?;
        Ok(TransferExitResp)
    }
}
