#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UdsDecodeError {
    TooShort { needed: usize },
    BadLength { expected: usize },
    InvalidFormat,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UdsPdu {
    pub len: u16,
    pub bytes: [u8; 4096],
}

impl UdsPdu {
    pub const fn empty() -> Self {
        Self {
            len: 0,
            bytes: [0; 4096],
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }

    pub fn from_response(bytes: &[u8]) -> Result<Self, NegativeResponse> {
        let mut pdu = Self::empty();
        pdu.len = bytes.len() as u16;
        pdu.bytes[..bytes.len()].copy_from_slice(bytes);

        if pdu.len >= 4 && pdu.bytes[1] == SID_NEG_RESPONSE {
            Err(NegativeResponse {
                service: pdu.bytes[2],
                code: UdsErrorCode::from_u8(pdu.bytes[3]),
            })
        } else {
            Ok(pdu)
        }
    }
}

fn pdu_set_header(pdu: &mut UdsPdu, sid: u8) {
    pdu.bytes[0] = DIVE_CAN_UDS_ADDR;
    pdu.bytes[1] = sid;
    pdu.len = 2;
}

fn pdu_push_payload(pdu: &mut UdsPdu, payload: &[u8]) {
    let start = pdu.len as usize;
    debug_assert!(start + payload.len() <= pdu.bytes.len());
    pdu.bytes[start..start + payload.len()].copy_from_slice(payload);
    pdu.len = (start + payload.len()) as u16;
}

fn ensure_len(pdu: &UdsPdu, needed: usize) -> Result<(), UdsDecodeError> {
    if (pdu.len as usize) < needed {
        Err(UdsDecodeError::TooShort { needed })
    } else {
        Ok(())
    }
}

pub fn pdu_sid(pdu: &UdsPdu) -> Result<u8, UdsDecodeError> {
    ensure_len(pdu, 2)?;
    Ok(pdu.bytes[1])
}

/// Generic negative response representation
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

    fn encode_request(req: &Self::Request<'_>) -> UdsPdu;
    fn decode_request<'a>(pdu: &'a UdsPdu) -> Result<Self::Request<'a>, UdsDecodeError>;

    fn encode_response(resp: &Self::Response<'_>) -> UdsPdu;
    fn decode_response<'a>(pdu: &'a UdsPdu) -> Result<Self::Response<'a>, UdsDecodeError>;
}

// ReadByIdentifier
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

    fn encode_request(req: &Self::Request<'_>) -> UdsPdu {
        let mut pdu = UdsPdu::empty();
        pdu_set_header(&mut pdu, Self::REQ_SID);
        pdu_push_payload(&mut pdu, &req.did.to_be_bytes());
        pdu
    }

    fn decode_request(pdu: &UdsPdu) -> Result<Self::Request<'_>, UdsDecodeError> {
        ensure_len(pdu, 4)?;
        if pdu.bytes[1] != Self::REQ_SID {
            return Err(UdsDecodeError::InvalidFormat);
        }
        let did = u16::from_be_bytes([pdu.bytes[2], pdu.bytes[3]]);
        Ok(ReadByIdentifierReq { did })
    }

    fn encode_response(resp: &Self::Response<'_>) -> UdsPdu {
        let mut pdu = UdsPdu::empty();
        pdu_set_header(&mut pdu, Self::RESP_SID);
        pdu_push_payload(&mut pdu, &resp.did.to_be_bytes());
        pdu_push_payload(&mut pdu, resp.data);
        pdu
    }

    fn decode_response<'a>(pdu: &'a UdsPdu) -> Result<Self::Response<'a>, UdsDecodeError> {
        ensure_len(pdu, 4)?;
        if pdu.bytes[1] != Self::RESP_SID {
            return Err(UdsDecodeError::InvalidFormat);
        }
        let did = u16::from_be_bytes([pdu.bytes[2], pdu.bytes[3]]);
        let data = &pdu.bytes[4..pdu.len as usize];
        Ok(ReadByIdentifierResp { did, data })
    }
}

// WriteByIdentifier
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

    fn encode_request(req: &Self::Request<'_>) -> UdsPdu {
        let mut pdu = UdsPdu::empty();
        pdu_set_header(&mut pdu, Self::REQ_SID);
        pdu_push_payload(&mut pdu, &req.did.to_be_bytes());
        pdu_push_payload(&mut pdu, req.data);
        pdu
    }

    fn decode_request<'a>(pdu: &'a UdsPdu) -> Result<Self::Request<'a>, UdsDecodeError> {
        ensure_len(pdu, 4)?;
        if pdu.bytes[1] != Self::REQ_SID {
            return Err(UdsDecodeError::InvalidFormat);
        }
        let did = u16::from_be_bytes([pdu.bytes[2], pdu.bytes[3]]);
        let data = &pdu.bytes[4..pdu.len as usize];
        Ok(WriteByIdentifierReq { did, data })
    }

    fn encode_response(resp: &Self::Response<'_>) -> UdsPdu {
        let mut pdu = UdsPdu::empty();
        pdu_set_header(&mut pdu, Self::RESP_SID);
        pdu_push_payload(&mut pdu, &resp.did.to_be_bytes());
        pdu
    }

    fn decode_response<'a>(pdu: &'a UdsPdu) -> Result<Self::Response<'a>, UdsDecodeError> {
        ensure_len(pdu, 4)?;
        if pdu.bytes[1] != Self::RESP_SID {
            return Err(UdsDecodeError::InvalidFormat);
        }
        let did = u16::from_be_bytes([pdu.bytes[2], pdu.bytes[3]]);
        Ok(WriteByIdentifierResp { did })
    }
}

// RequestDownload
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

    fn encode_request(req: &Self::Request<'_>) -> UdsPdu {
        let mut pdu = UdsPdu::empty();
        pdu_set_header(&mut pdu, Self::REQ_SID);
        pdu_push_payload(&mut pdu, &[DFI_PLAIN, ALFI_ADDR4_SIZE4]);
        pdu_push_payload(&mut pdu, &req.address.to_be_bytes());
        pdu_push_payload(&mut pdu, &req.size.to_be_bytes());
        pdu
    }

    fn decode_request(pdu: &UdsPdu) -> Result<Self::Request<'_>, UdsDecodeError> {
        ensure_len(pdu, 12)?;
        if pdu.bytes[1] != Self::REQ_SID {
            return Err(UdsDecodeError::InvalidFormat);
        }
        if pdu.bytes[2] != DFI_PLAIN || pdu.bytes[3] != ALFI_ADDR4_SIZE4 {
            return Err(UdsDecodeError::InvalidFormat);
        }
        let address = u32::from_be_bytes([pdu.bytes[4], pdu.bytes[5], pdu.bytes[6], pdu.bytes[7]]);
        let size = u32::from_be_bytes([pdu.bytes[8], pdu.bytes[9], pdu.bytes[10], pdu.bytes[11]]);
        Ok(RequestDownloadReq { address, size })
    }

    fn encode_response(resp: &Self::Response<'_>) -> UdsPdu {
        let mut pdu = UdsPdu::empty();
        pdu_set_header(&mut pdu, Self::RESP_SID);
        pdu_push_payload(&mut pdu, resp.payload);
        pdu
    }

    fn decode_response<'a>(pdu: &'a UdsPdu) -> Result<Self::Response<'a>, UdsDecodeError> {
        ensure_len(pdu, 2)?;
        if pdu.bytes[1] != Self::RESP_SID {
            return Err(UdsDecodeError::InvalidFormat);
        }
        let payload = &pdu.bytes[2..pdu.len as usize];
        Ok(RequestDownloadResp { payload })
    }
}

// RequestUpload
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

    fn encode_request(req: &Self::Request<'_>) -> UdsPdu {
        let mut pdu = UdsPdu::empty();
        pdu_set_header(&mut pdu, Self::REQ_SID);
        pdu_push_payload(&mut pdu, &[DFI_PLAIN, ALFI_ADDR4_SIZE4]);
        pdu_push_payload(&mut pdu, &req.address.to_be_bytes());
        pdu_push_payload(&mut pdu, &req.size.to_be_bytes());
        pdu
    }

    fn decode_request(pdu: &UdsPdu) -> Result<Self::Request<'_>, UdsDecodeError> {
        ensure_len(pdu, 12)?;
        if pdu.bytes[1] != Self::REQ_SID {
            return Err(UdsDecodeError::InvalidFormat);
        }
        if pdu.bytes[2] != DFI_PLAIN || pdu.bytes[3] != ALFI_ADDR4_SIZE4 {
            return Err(UdsDecodeError::InvalidFormat);
        }
        let address = u32::from_be_bytes([pdu.bytes[4], pdu.bytes[5], pdu.bytes[6], pdu.bytes[7]]);
        let size = u32::from_be_bytes([pdu.bytes[8], pdu.bytes[9], pdu.bytes[10], pdu.bytes[11]]);
        Ok(RequestUploadReq { address, size })
    }

    fn encode_response(resp: &Self::Response<'_>) -> UdsPdu {
        let mut pdu = UdsPdu::empty();
        pdu_set_header(&mut pdu, Self::RESP_SID);
        pdu_push_payload(&mut pdu, resp.payload);
        pdu
    }

    fn decode_response<'a>(pdu: &'a UdsPdu) -> Result<Self::Response<'a>, UdsDecodeError> {
        ensure_len(pdu, 2)?;
        if pdu.bytes[1] != Self::RESP_SID {
            return Err(UdsDecodeError::InvalidFormat);
        }
        let payload = &pdu.bytes[2..pdu.len as usize];
        Ok(RequestUploadResp { payload })
    }
}

// TransferData
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

    fn encode_request(req: &Self::Request<'_>) -> UdsPdu {
        let mut pdu = UdsPdu::empty();
        pdu_set_header(&mut pdu, Self::REQ_SID);
        pdu_push_payload(&mut pdu, &[req.block_seq]);
        pdu_push_payload(&mut pdu, req.payload);
        pdu
    }

    fn decode_request<'a>(pdu: &'a UdsPdu) -> Result<Self::Request<'a>, UdsDecodeError> {
        ensure_len(pdu, 3)?;
        if pdu.bytes[1] != Self::REQ_SID {
            return Err(UdsDecodeError::InvalidFormat);
        }
        let block_seq = pdu.bytes[2];
        let payload = &pdu.bytes[3..pdu.len as usize];
        Ok(TransferDataReq { block_seq, payload })
    }

    fn encode_response(resp: &Self::Response<'_>) -> UdsPdu {
        let mut pdu = UdsPdu::empty();
        pdu_set_header(&mut pdu, Self::RESP_SID);
        pdu_push_payload(&mut pdu, &[resp.block_seq]);
        pdu_push_payload(&mut pdu, resp.payload);
        pdu
    }

    fn decode_response<'a>(pdu: &'a UdsPdu) -> Result<Self::Response<'a>, UdsDecodeError> {
        ensure_len(pdu, 3)?;
        if pdu.bytes[1] != Self::RESP_SID {
            return Err(UdsDecodeError::InvalidFormat);
        }
        let block_seq = pdu.bytes[2];
        let payload = &pdu.bytes[3..pdu.len as usize];
        Ok(TransferDataResp { block_seq, payload })
    }
}

// TransferExit
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

    fn encode_request(_: &Self::Request<'_>) -> UdsPdu {
        let mut pdu = UdsPdu::empty();
        pdu_set_header(&mut pdu, Self::REQ_SID);
        pdu
    }

    fn decode_request(pdu: &UdsPdu) -> Result<Self::Request<'_>, UdsDecodeError> {
        ensure_len(pdu, 2)?;
        if pdu.bytes[1] != Self::REQ_SID {
            return Err(UdsDecodeError::InvalidFormat);
        }
        Ok(TransferExitReq)
    }

    fn encode_response(_: &Self::Response<'_>) -> UdsPdu {
        let mut pdu = UdsPdu::empty();
        pdu_set_header(&mut pdu, Self::RESP_SID);
        pdu
    }

    fn decode_response<'a>(pdu: &'a UdsPdu) -> Result<Self::Response<'a>, UdsDecodeError> {
        ensure_len(pdu, 2)?;
        if pdu.bytes[1] != Self::RESP_SID {
            return Err(UdsDecodeError::InvalidFormat);
        }
        Ok(TransferExitResp)
    }
}
