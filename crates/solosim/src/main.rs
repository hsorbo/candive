use candive::diag::settings::SettingValue;
use candive::diag::settings::UserSettingDid;
use candive::diag::settings::UserSettingPayload;
use candive::diag::settings::UserSettingType;
use candive::divecan;
use candive::divecan::DiveCanFrame;
use candive::uds::client;
use candive::uds::client::ProtocolError;
use candive::uds::client::UdsClientError;
use candive::uds::isotp;
use candive::uds::isotp::IsoTpPciType;
use candive::uds::isotp::IsoTpRx;
use candive::uds::isotp::IsoTpRxError;
use candive::uds::isotp::IsoTpRxEvent;
use candive::uds::uds::{
    ReadByIdentifierCodec, SID_RDBI_REQ, SID_WDBI_REQ, ServiceCodec, UdsErrorCode, UdsPduView,
    UdsPduWriter, WriteByIdentifierCodec,
};
use socketcan::CanFrame;
use socketcan::CanSocket;
use socketcan::EmbeddedFrame;
use socketcan::ExtendedId;
use socketcan::Id;
use socketcan::Socket;

use candive::divecan::DiveCanId;
use candive::divecan::Msg;
use candive::units::{CentiMillivolt, Decivolt, Milliamp, Millisecond, PpO2Deci};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "solosim")]
#[command(about = "Solo simulator CLI tool", long_about = None)]
struct Args {
    /// Mode to run: "menu" or "simulator"
    #[arg(short, long, default_value = "simulator")]
    mode: String,

    /// CAN device to use
    #[arg(short, long, default_value = "can0")]
    device: String,
}

// ============================================================================
// Menu mode implementation (from menu.rs)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    IsoTp(IsoTpRxError),
    Io,
}

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

pub struct SocketCanCustomIsoTpUdsSession<'a> {
    socket: &'a socketcan::CanSocket,
    id: &'a DiveCanId,
}

impl<'a> SocketCanCustomIsoTpUdsSession<'a> {
    pub fn new(socket: &'a socketcan::CanSocket, id: &'a DiveCanId) -> Self {
        Self { socket, id }
    }

    pub fn send_isoptp(&self, data: &[u8]) -> Result<(), UdsClientError<TransportError>> {
        let segmenter = isotp::IsoTpTx::new(&data);
        for (i, segment) in segmenter.enumerate() {
            if i == 1 {
                let _ = self
                    .socket
                    .read_frame()
                    .map_err(|_| UdsClientError::Transport(TransportError::Io))?;
            }

            let reply_id = DiveCanId {
                src: self.id.dst,
                dst: self.id.src,
                kind: self.id.kind,
            };

            let ext = socketcan::ExtendedId::new(reply_id.to_u32())
                .ok_or_else(|| ProtocolError::UnexpectedResponse)?;
            let c = socketcan::CanFrame::new(ext, segment.as_slice())
                .ok_or_else(|| ProtocolError::UnexpectedResponse)?;
            self.socket
                .write_frame(&c)
                .map_err(|_| UdsClientError::Transport(TransportError::Io))?;
        }
        Ok(())
    }

    fn recv_isoptp(
        &self,
        expected_id: Option<DiveCanId>,
    ) -> Result<Vec<u8>, UdsClientError<TransportError>> {
        let mut rx = IsoTpRx::new();

        loop {
            let frame = &self
                .socket
                .read_frame()
                .map_err(|_| UdsClientError::Transport(TransportError::Io))?;

            let socketcan::Id::Extended(extended_id) = frame.id() else {
                continue; // Skip standard IDs
            };

            let raw_id = extended_id.as_raw();
            let rx_id: DiveCanId = raw_id.into();
            if rx_id.kind != 0xa {
                continue;
            }

            if let Some(expected_id) = expected_id {
                if rx_id.src != expected_id.dst
                    || rx_id.dst != expected_id.src
                    || rx_id.kind != expected_id.kind
                {
                    continue;
                }
            }

            let data = frame.data();
            let len = data.len();
            if len == 0 || len > 8 {
                continue;
            }

            let mut buf = [0u8; 8];
            buf[..len].copy_from_slice(data);

            match rx.on_frame(&buf[..len]) {
                Ok(IsoTpRxEvent::Completed(total_len)) => {
                    let mut out = vec![0u8; total_len];
                    out.copy_from_slice(&rx.payload()[..total_len]);
                    return Ok(out);
                }
                Ok(IsoTpRxEvent::FlowControlRequired) => {
                    let reply_id = rx_id.reply(rx_id.kind);
                    let ext = socketcan::ExtendedId::new(reply_id.to_u32())
                        .ok_or_else(|| ProtocolError::UnexpectedResponse)?;

                    let fc = isotp::make_flow_control_cts(0, 0);

                    let c = socketcan::CanFrame::new(ext, fc.as_slice())
                        .ok_or_else(|| ProtocolError::UnexpectedResponse)?;
                    self.socket
                        .write_frame(&c)
                        .map_err(|_| UdsClientError::Transport(TransportError::Io))?;
                    continue;
                }
                Ok(IsoTpRxEvent::None) => {
                    continue;
                }
                Err(err) => {
                    if let IsoTpRxError::UnexpectedFrameType { expected: _, got } = err {
                        if got == IsoTpPciType::FlowControl {
                            continue;
                        }
                    }
                    rx.reset();
                    return Err(UdsClientError::Transport(err.into()));
                }
            }
        }
    }
}

impl<'a> client::UdsTransport for SocketCanCustomIsoTpUdsSession<'a> {
    type Error = TransportError;

    fn request(&mut self, req: &[u8], resp_buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.send_isoptp(req).map_err(|e| match e {
            UdsClientError::Transport(t) => t,
            _ => TransportError::Io, // Fallback for protocol/other errors
        })?;
        let resp = self.recv_isoptp(None).map_err(|e| match e {
            UdsClientError::Transport(t) => t,
            _ => TransportError::Io,
        })?;
        if resp.len() > resp_buf.len() {
            return Err(TransportError::Io); // Could add ResponseTooLarge variant
        }
        resp_buf[..resp.len()].copy_from_slice(&resp);
        Ok(resp.len())
    }
}

#[derive(Debug)]
struct Entry {
    key: &'static [u8; 10],
    value: SettingValue,
    vals: Option<&'static [&'static [u8; 8]]>,
}

const MENU: &[Entry] = &[
    Entry {
        key: b"P_SIZE    ",
        value: SettingValue::SelectionIndex {
            max_index: 6,
            current_index: 1,
        },
        vals: Some(&[
            b"8D      ",
            b"8=D     ",
            b"8==D    ",
            b"8===D   ",
            b"8====D  ",
            b"8=====D ",
            b"8======D",
        ]),
    },
    Entry {
        key: b"DEV_NAME  ",
        value: SettingValue::IntegerHex {
            value: 0x0B00B1E5,
            min: 0x0B00B1E5 - 5,
            max: 0x0B00B1E5 + 5,
        },
        vals: None,
    },
    Entry {
        key: b"TEMP_C    ",
        value: SettingValue::IntegerScaled {
            value: 7200,
            divisor: 100,
            min: 7000,
            max: 8000,
        },
        vals: None,
    },
];

fn handle_menu_read(udid: UserSettingDid) -> UserSettingPayload {
    match udid {
        UserSettingDid::Count => UserSettingPayload::Count(MENU.len() as u8),
        UserSettingDid::Info { index } => {
            let entry = &MENU[index as usize];
            let kind = match &entry.value {
                SettingValue::SelectionIndex { .. } => UserSettingType::Selection,
                SettingValue::IntegerHex { .. } => UserSettingType::Integer,
                SettingValue::IntegerScaled { .. } => UserSettingType::Scaled,
            };
            UserSettingPayload::Info {
                name: *entry.key,
                editable: true,
                kind,
            }
        }
        UserSettingDid::ReadState { index } => {
            let entry = &MENU[index as usize];
            UserSettingPayload::State(entry.value.encode())
        }
        UserSettingDid::Enum { enum_index, index } => {
            let entry = &MENU[index as usize];
            UserSettingPayload::Enum(*entry.vals.unwrap()[enum_index as usize])
        }
        UserSettingDid::WriteInput { .. } => {
            todo!("Shouldn't be read here, only written")
        }
    }
}

fn process_uds_request(req_data: &[u8], resp_buf: &mut [u8]) -> usize {
    let req_view = UdsPduView::new(req_data);

    match req_view.sid().unwrap_or(0) {
        SID_RDBI_REQ => {
            if let Ok(req) = ReadByIdentifierCodec::decode_request(req_view) {
                match UserSettingDid::try_from(req.did) {
                    Ok(udid) => {
                        let response = handle_menu_read(udid);
                        let mut buf = [0u8; 100];
                        let len = response.encode(&mut buf).unwrap();
                        let resp = candive::uds::uds::ReadByIdentifierResp {
                            did: req.did,
                            data: &buf[..len],
                        };
                        let mut writer = UdsPduWriter::new(resp_buf);
                        ReadByIdentifierCodec::encode_response(&resp, &mut writer).unwrap();
                        writer.len()
                    }
                    Err(_) => {
                        let writer = UdsPduWriter::make_negative_response(
                            resp_buf,
                            SID_RDBI_REQ,
                            UdsErrorCode::IncorrectMessageLengthOrInvalidFormat,
                        )
                        .unwrap();
                        writer.len()
                    }
                }
            } else {
                let writer = UdsPduWriter::make_negative_response(
                    resp_buf,
                    SID_RDBI_REQ,
                    UdsErrorCode::IncorrectMessageLengthOrInvalidFormat,
                )
                .unwrap();
                writer.len()
            }
        }
        SID_WDBI_REQ => {
            if let Ok(req) = WriteByIdentifierCodec::decode_request(req_view) {
                println!(
                    "WriteByIdentifierRequest: {:x}: {}",
                    req.did,
                    hex::encode(req.data)
                );
                let resp = candive::uds::uds::WriteByIdentifierResp { did: req.did };
                let mut writer = UdsPduWriter::new(resp_buf);
                WriteByIdentifierCodec::encode_response(&resp, &mut writer).unwrap();
                writer.len()
            } else {
                let writer = UdsPduWriter::make_negative_response(
                    resp_buf,
                    SID_WDBI_REQ,
                    UdsErrorCode::IncorrectMessageLengthOrInvalidFormat,
                )
                .unwrap();
                writer.len()
            }
        }
        _ => {
            println!("Not implemented");
            let writer =
                UdsPduWriter::make_negative_response(resp_buf, 0, UdsErrorCode::GeneralReject)
                    .unwrap();
            writer.len()
        }
    }
}

fn run_menu_mode(device: &str) -> anyhow::Result<()> {
    let socket = CanSocket::open(device)?;
    let my_id = 8;
    println!("Running in MENU mode on {}...", device);
    let mut rx = IsoTpRx::new();

    loop {
        let frame = socket.read_frame()?;
        let Id::Extended(extended_id) = frame.id() else {
            panic!("Standard IDs not supported");
        };

        let id: DiveCanId = extended_id.as_raw().into();

        let data = frame.data();
        let mut payload = [0u8; 8];
        let len = data.len().min(8);
        payload[..len].copy_from_slice(&data[..len]);
        println!("kind: {}, dlc: {}", id.kind, frame.dlc());
        let dc_frame = DiveCanFrame::new(id.kind, frame.dlc() as u8, payload).unwrap();
        let msg = Msg::try_from_frame(&dc_frame).unwrap();

        match msg {
            Msg::Undocumented30 { .. } | Msg::Id { .. } => {
                let sendlist = vec![
                    Msg::Id {
                        manufacturer: 1,
                        version: 0,
                        unused: 0,
                    },
                    Msg::DeviceName(*b"MENUDEMO"),
                ];
                for x in sendlist {
                    let id = DiveCanId::new(my_id, 0, x.kind());
                    let frame = &x.to_frame();
                    let ext = ExtendedId::new(id.to_u32()).unwrap();
                    let can_frame = CanFrame::new(ext, &frame.bytes()).unwrap();
                    socket.write_frame(&can_frame).unwrap();
                }
            }
            Msg::Uds { dlc, data } => {
                if id.dst != my_id {
                    continue;
                }
                match rx.on_frame(&data[..dlc as usize]) {
                    Ok(event) => match event {
                        IsoTpRxEvent::Completed(total_len) => {
                            // Copy the request data before resetting rx
                            let mut req_buf = [0u8; 4096];
                            req_buf[..total_len].copy_from_slice(&rx.payload()[..total_len]);
                            rx.reset();

                            let mut resp_buf = [0u8; 4096];
                            let resp_len =
                                process_uds_request(&req_buf[..total_len], &mut resp_buf);

                            let isotp = SocketCanCustomIsoTpUdsSession::new(&socket, &id);
                            isotp.send_isoptp(&resp_buf[..resp_len]).unwrap();
                        }
                        IsoTpRxEvent::FlowControlRequired => {
                            let reply_id = id.reply(id.kind);
                            let ext = ExtendedId::new(reply_id.to_u32()).unwrap();

                            let fc = isotp::make_flow_control_cts(0, 0);

                            let c = CanFrame::new(ext, fc.as_slice()).unwrap();
                            socket.write_frame(&c).unwrap();
                        }
                        IsoTpRxEvent::None => {}
                    },
                    Err(err) => {
                        println!("Error: {:?}", err);
                        rx.reset();
                    }
                }
            }
            _ => {}
        }
    }
}

// ============================================================================
// Simulator mode implementation (from solosimulator.rs)
// ============================================================================

fn sendlist() -> Vec<Msg> {
    let lst = vec![
        Msg::Id {
            manufacturer: 1,
            version: 0,
            unused: 0,
        },
        Msg::DeviceName(*b"YOLO\0\0\0\0"),
        Msg::CellVoltages {
            cell_voltages: [
                CentiMillivolt::new(100),
                CentiMillivolt::new(100),
                CentiMillivolt::new(100),
            ],
            unused: 0,
        },
        Msg::CellPpo2([PpO2Deci::new(120), PpO2Deci::new(125), PpO2Deci::new(122)]),
        Msg::CellStatus {
            cells_active: divecan::CellsActive::new([true, true, true]),
            consensus: divecan::Consensus::PpO2(PpO2Deci::new(100)),
        },
        Msg::SoloStatus {
            current: Milliamp::new(0),
            injection_duration: Millisecond::new(0),
            voltage: Decivolt::new(70),
            setpoint: PpO2Deci::new(0x70),
            consensus: divecan::Consensus::PpO2(PpO2Deci::new(0x44)),
            voltage_alert: None,
            current_alert: None,
        },
    ];
    lst
}

fn to_can_frame(id: DiveCanId, msg: divecan::Msg) -> CanFrame {
    let frame = &msg.to_frame();
    let ext = ExtendedId::new(id.to_u32()).unwrap();
    CanFrame::new(ext, &frame.bytes()).unwrap()
}

fn run_simulator_mode(device: &str) -> anyhow::Result<()> {
    let socket = CanSocket::open(device)?;
    println!("Running in SIMULATOR mode on {}...", device);

    let msg = sendlist()[0];
    let id = DiveCanId::new(4, 0, msg.kind());
    let frame = to_can_frame(id, msg);
    socket.write_frame(&frame).unwrap();

    loop {
        let frame = socket.read_frame()?;

        let Id::Extended(extended_id) = frame.id() else {
            panic!("Standard IDs not supported");
        };

        let id: DiveCanId = extended_id.as_raw().into();

        let data = frame.data();
        let mut payload = [0u8; 8];
        let len = data.len().min(8);
        payload[..len].copy_from_slice(&data[..len]);
        let dc_frame = DiveCanFrame::new(id.kind, frame.dlc() as u8, payload).unwrap();
        let msg = Msg::try_from_frame(&dc_frame).unwrap();

        match msg {
            Msg::Id { .. } => {
                for x in sendlist() {
                    let id = DiveCanId::new(2, 0, x.kind());
                    socket.write_frame(&to_can_frame(id, x)).unwrap();
                }
            }
            _ => {}
        }
    }
}

// ============================================================================
// Main entry point
// ============================================================================

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.mode.as_str() {
        "menu" => run_menu_mode(&args.device),
        "simulator" => run_simulator_mode(&args.device),
        _ => {
            eprintln!("Invalid mode: {}. Use 'menu' or 'simulator'.", args.mode);
            std::process::exit(1);
        }
    }
}
