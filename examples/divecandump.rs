use candive::divecan;
use candive::divecan::DiveCanFrame;
use candive::uds::isotp::IsoTpRx;
use candive::uds::isotp::IsoTpRxEvent;
use socketcan::CanSocket;
use socketcan::EmbeddedFrame;
use socketcan::Id;
use socketcan::Socket;

use crate::divecan::DiveCanId;
use crate::divecan::Msg;

use std::collections::HashMap;
use std::env;
use std::fmt;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SessionKey {
    src: u8,
    dst: u8,
}

impl From<DiveCanId> for SessionKey {
    fn from(id: DiveCanId) -> Self {
        Self {
            src: id.src,
            dst: id.dst,
        }
    }
}

pub struct HexSlice<'a>(pub &'a [u8]);

impl<'a> fmt::Debug for HexSlice<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, b) in self.0.iter().enumerate() {
            if i > 0 {
                write!(f, "")?;
            }
            write!(f, "{:02X}", b)?;
        }
        Ok(())
    }
}

fn b2(two: &[u8]) -> u8 {
    fn hb(b: u8) -> u8 {
        match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            _ => unreachable!(),
        }
    }
    (hb(two[0]) << 4) | hb(two[1])
}

fn handle_frame(
    id: DiveCanId,
    dlc: u8,
    payload: &[u8; 8],
    sessions: &mut HashMap<SessionKey, IsoTpRx>,
) {
    let dc_frame = DiveCanFrame::new(id.kind, dlc, *payload).unwrap();
    let msg = Msg::try_from_frame(&dc_frame).unwrap();

    match msg {
        Msg::Uds { dlc, data } => {
            let session_key: SessionKey = id.into();
            let rx = sessions.entry(session_key).or_insert_with(IsoTpRx::new);

            match rx.on_frame(&data[..dlc as usize]) {
                Ok(IsoTpRxEvent::Completed(total_len)) => {
                    let mut out = vec![0u8; total_len];
                    out.copy_from_slice(&rx.payload()[..total_len]);
                    println!(
                        "{:x} -> {:x} {:02x} UDS: {:?}",
                        id.src,
                        id.dst,
                        id.kind,
                        HexSlice(&out)
                    );
                    rx.reset();
                }
                Ok(IsoTpRxEvent::FlowControlRequired) => {}
                Ok(IsoTpRxEvent::None) => {}
                Err(err) => {
                    println!("Error: {:?}", err);
                    rx.reset();
                }
            }
        }
        _ => {
            println!("{:x} -> {:x} {:02x}, {:?}", id.src, id.dst, id.kind, msg);
        }
    }
}

fn dumplive() -> anyhow::Result<()> {
    let socket = CanSocket::open("can0")?;
    println!("Listening on can0...");
    let mut sessions = HashMap::new();

    loop {
        let frame = socket.read_frame()?;

        let Id::Extended(extended_id) = frame.id() else {
            println!("Standard IDs not supported");
            continue;
        };

        let id: DiveCanId = extended_id.as_raw().into();

        let data = frame.data();
        let mut payload = [0u8; 8];
        let len = data.len().min(8);
        payload[..len].copy_from_slice(&data[..len]);

        handle_frame(id, frame.dlc() as u8, &payload, &mut sessions);
    }
}

fn dumpfile(path: String) {
    let f = BufReader::new(File::open(path).unwrap());
    let mut sessions = HashMap::new();

    for line in f.lines() {
        let s = line.unwrap();
        if s.trim().is_empty() {
            continue;
        }

        // "(030.026910) can0 0D010004#432D696E61746F72"
        let (_ts_part, rest) = s.split_once(')').unwrap();

        let mut it = rest.trim().split_whitespace();
        let _iface = it.next().unwrap();
        let id_data = it.next().unwrap();
        let (id_hex, data_hex) = id_data.split_once('#').unwrap();

        let id = u32::from_str_radix(id_hex, 16).unwrap();

        let db = data_hex.as_bytes();
        let dlc = db.len() / 2;
        let mut data = [0u8; 8];
        for i in 0..dlc {
            data[i] = b2(&db[i * 2..i * 2 + 2]);
        }
        let did: DiveCanId = id.into();

        handle_frame(did, dlc as u8, &data, &mut sessions);
    }
}

fn main() -> anyhow::Result<()> {
    match env::args().nth(1) {
        Some(path) => {
            dumpfile(path);
            Ok(())
        }
        None => dumplive(),
    }
}
