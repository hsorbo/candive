use super::uds::*;

pub trait UdsTransport {
    type Error;

    fn request(&mut self, req: &[u8], resp_buf: &mut [u8]) -> Result<usize, Self::Error>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UdsClientError<E> {
    Transport(E),
    NegativeResponse(NegativeResponse),
    Decode(UdsDecodeError),
    Encode(UdsEncodeError),
    Protocol(ProtocolError),
    ResponseTooLarge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    WrongDid { expected: u16, got: u16 },
    WrongBlockCounter { expected: u8, got: u8 },
    EmptyPayload,
    UnexpectedResponse,
}

impl<E> From<UdsEncodeError> for UdsClientError<E> {
    fn from(e: UdsEncodeError) -> Self {
        UdsClientError::Encode(e)
    }
}

impl<E> From<UdsDecodeError> for UdsClientError<E> {
    fn from(e: UdsDecodeError) -> Self {
        UdsClientError::Decode(e)
    }
}

impl<E> From<NegativeResponse> for UdsClientError<E> {
    fn from(e: NegativeResponse) -> Self {
        UdsClientError::NegativeResponse(e)
    }
}

impl<E> From<ProtocolError> for UdsClientError<E> {
    fn from(err: ProtocolError) -> Self {
        UdsClientError::Protocol(err)
    }
}

fn transact<'a, C: ServiceCodec, T: UdsTransport>(
    transport: &mut T,
    tx_buf: &mut [u8],
    rx_buf: &'a mut [u8],
    req: &C::Request<'_>,
) -> Result<C::Response<'a>, UdsClientError<T::Error>> {
    let mut writer = UdsPduWriter::new(tx_buf);
    C::encode_request(req, &mut writer)?;

    let resp_len = transport
        .request(writer.as_bytes(), rx_buf)
        .map_err(UdsClientError::Transport)?;

    if resp_len > rx_buf.len() {
        return Err(UdsClientError::ResponseTooLarge);
    }

    let view = UdsPduView::new(&rx_buf[..resp_len]);
    view.check_positive()?;

    Ok(C::decode_response(view)?)
}

pub fn rdbi<'rx, T: UdsTransport>(
    transport: &mut T,
    did: u16,
    tx_buf: &mut [u8],
    rx_buf: &'rx mut [u8],
) -> Result<&'rx [u8], UdsClientError<T::Error>> {
    let req = ReadByIdentifierReq { did };
    let resp = transact::<ReadByIdentifierCodec, _>(transport, tx_buf, rx_buf, &req)?;

    if resp.did != did {
        return Err(ProtocolError::WrongDid {
            expected: did,
            got: resp.did,
        }
        .into());
    }

    Ok(resp.data)
}

pub fn wdbi<T: UdsTransport>(
    transport: &mut T,
    did: u16,
    data: &[u8],
    tx_buf: &mut [u8],
    rx_buf: &mut [u8],
) -> Result<(), UdsClientError<T::Error>> {
    let req = WriteByIdentifierReq { did, data };
    let resp = transact::<WriteByIdentifierCodec, _>(transport, tx_buf, rx_buf, &req)?;

    if resp.did != did {
        return Err(ProtocolError::WrongDid {
            expected: did,
            got: resp.did,
        }
        .into());
    }

    Ok(())
}
pub struct DownloadSession<'a, T: UdsTransport> {
    transport: &'a mut T,
    tx_buf: &'a mut [u8],
    rx_buf: &'a mut [u8],
    max_block_len: usize,
    next_block: u8,
}

impl<'a, T: UdsTransport> DownloadSession<'a, T> {
    pub fn start(
        transport: &'a mut T,
        address: u32,
        size: u32,
        tx_buf: &'a mut [u8],
        rx_buf: &'a mut [u8],
    ) -> Result<Self, UdsClientError<T::Error>> {
        let req = RequestDownloadReq { address, size };
        let resp = transact::<RequestDownloadCodec, _>(transport, tx_buf, rx_buf, &req)?;

        if resp.payload.is_empty() {
            return Err(ProtocolError::EmptyPayload.into());
        }
        let max_block_len = resp.payload[0] as usize;

        Ok(Self {
            transport,
            tx_buf,
            rx_buf,
            max_block_len,
            next_block: 1,
        })
    }

    pub fn max_block_len(&self) -> usize {
        self.max_block_len
    }

    pub fn send_block(&mut self, data: &[u8]) -> Result<(), UdsClientError<T::Error>> {
        let req = TransferDataReq {
            block_seq: self.next_block,
            payload: data,
        };
        let resp =
            transact::<TransferDataCodec, _>(self.transport, self.tx_buf, self.rx_buf, &req)?;

        if resp.block_seq != self.next_block {
            return Err(ProtocolError::WrongBlockCounter {
                expected: self.next_block,
                got: resp.block_seq,
            }
            .into());
        }

        self.next_block = self.next_block.wrapping_add(1);

        Ok(())
    }

    pub fn finish(self) -> Result<(), UdsClientError<T::Error>> {
        let _ = transact::<TransferExitCodec, _>(
            self.transport,
            self.tx_buf,
            self.rx_buf,
            &TransferExitReq,
        )?;
        Ok(())
    }
}

pub struct UploadSession<'a, T: UdsTransport> {
    transport: &'a mut T,
    tx_buf: &'a mut [u8],
    rx_buf: &'a mut [u8],
    next_block: u8,
    total_size: usize,
    transferred: usize,
}

impl<'a, T: UdsTransport> UploadSession<'a, T> {
    pub fn start(
        transport: &'a mut T,
        address: u32,
        size: u32,
        dlf: Dlf,
        tx_buf: &'a mut [u8],
        rx_buf: &'a mut [u8],
    ) -> Result<Self, UdsClientError<T::Error>> {
        let req = RequestUploadReq { dlf, address, size };
        let _resp = transact::<RequestUploadCodec, _>(transport, tx_buf, rx_buf, &req)?;

        Ok(Self {
            transport,
            tx_buf,
            rx_buf,
            next_block: 1,
            total_size: size as usize,
            transferred: 0,
        })
    }

    pub fn read_block(&mut self, out: &mut [u8]) -> Result<usize, UdsClientError<T::Error>> {
        if self.transferred >= self.total_size {
            return Ok(0);
        }

        let req = TransferDataReq {
            block_seq: self.next_block,
            payload: &[],
        };
        let resp =
            transact::<TransferDataCodec, _>(self.transport, self.tx_buf, self.rx_buf, &req)?;

        if resp.block_seq != self.next_block {
            return Err(ProtocolError::WrongBlockCounter {
                expected: self.next_block,
                got: resp.block_seq,
            }
            .into());
        }

        if resp.payload.is_empty() {
            return Ok(0);
        }

        let remaining = self.total_size - self.transferred;
        let to_copy = resp.payload.len().min(remaining).min(out.len());
        out[..to_copy].copy_from_slice(&resp.payload[..to_copy]);

        self.transferred += to_copy;
        self.next_block = self.next_block.wrapping_add(1);

        Ok(to_copy)
    }

    pub fn transferred(&self) -> usize {
        self.transferred
    }

    pub fn total_size(&self) -> usize {
        self.total_size
    }

    pub fn finish(self) -> Result<(), UdsClientError<T::Error>> {
        let _ = transact::<TransferExitCodec, _>(
            self.transport,
            self.tx_buf,
            self.rx_buf,
            &TransferExitReq,
        )?;
        Ok(())
    }
}
