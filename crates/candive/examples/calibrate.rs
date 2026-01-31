use candive::divecan;
use candive::divecan::DiveCanFrame;
use socketcan::CanFrame;
use socketcan::CanSocket;
use socketcan::EmbeddedFrame;
use socketcan::ExtendedId;
use socketcan::Id;
use socketcan::Socket;

use crate::divecan::DiveCanId;
use crate::divecan::Msg;

fn to_can_frame(id: DiveCanId, msg: divecan::Msg) -> CanFrame {
    let dc_frame = &msg.to_frame();
    let ext = ExtendedId::new(id.to_u32()).unwrap();
    CanFrame::new(ext, dc_frame.bytes()).unwrap()
}

fn main() -> anyhow::Result<()> {
    println!("Using can0");

    let socket = CanSocket::open("can0")?;
    let msg = Msg::Ppo2CalibrationRequest {
        fo2: 99.into(),
        pressure: 1003.into(),
    };
    let id = DiveCanId::new(1, 4, msg.kind());
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
        println!("msg {:?}", msg);
    }
}
