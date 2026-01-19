use candive::divecan::{DiveCanFrame, DiveCanId, Msg};
use std::env;
use std::fs::File;
use std::io::Read;

fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} [--divecan] <log.bin>", args[0]);
        std::process::exit(1);
    }

    let divecan_mode = args.contains(&"--divecan".to_string());
    let path = args
        .iter()
        .find(|a| !a.starts_with("--") && *a != &args[0])
        .unwrap();
    let mut file = File::open(path)?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;

    let mut kind = 0x00u8;
    let num_entries = data.len() / 12;

    for i in 0..num_entries {
        let start = i * 12;
        let end = start + 12;
        let entry = &data[start..end];

        if entry.iter().all(|&b| b == 0xFF) || entry.iter().all(|&b| b == 0x00) {
            kind = entry[10];
            continue;
        }

        let can_id = 0x0D000000u32 | ((kind as u32) << 16) | 0x0004;
        let dlc = Msg::dlc_min_size(kind).unwrap_or(8);
        let mut payload = [0u8; 8];
        let copy_len = dlc.min(8) as usize;
        payload[..copy_len].copy_from_slice(&entry[..copy_len]);

        if divecan_mode {
            if let Ok(frame) = DiveCanFrame::new(kind, dlc, payload) {
                if let Ok(msg) = Msg::try_from_frame(&frame) {
                    let id: DiveCanId = can_id.into();
                    println!("{:02x} -> {:02x}: {:?}", id.src, id.dst, msg);
                }
            }
        } else {
            let payload_str = entry[..dlc as usize]
                .iter()
                .map(|b| format!("{:02X}", b))
                .collect::<Vec<_>>()
                .join(" ");
            println!("  can0  {:08X}   [{}]  {}", can_id, dlc, payload_str);
        }

        kind = entry[10];
    }

    Ok(())
}

/*

#!/usr/bin/env python3
import sys

DLC = {
    0x00:3, # Id
    0x01:8, # DeviceName
    0x02:3, # Alert
    0x03:1, # ShutdownInit
    0x04:4, # CellPpo2
    0x07:5, # OboeStatus
    0x08:5, # AmbientPressure
    0x0A:8, # Uds
    0x0B:3, # TankPressure
    0x10:8, # Nop
    0x11:7, # CellVoltages
    0x12:8, # Ppo2CalibrationResponse
    0x13:3, # Ppo2CalibrationRequest
    0x20:1, # Co2Enabled
    0x21:3, # Co2
    0x22:3, # Co2CalibrationResponse
    0x23:2, # Co2CalibrationRequest
    0x30:3, # Undocumented30
    0x37:3, # BusInit
    0xC1:3, # TempProbe
    0xC3:6, # UndocumentedC3
    0xC4:1, # TempProbeEnabled
    0xC9:1, # Setpoint
    0xCA:2, # CellStatus
    0xCB:8, # SoloStatus
    0xCC:7, # Diving
    0xD2:8  # Serial
}

data = open(sys.argv[1], 'rb').read()
kind = 0x00
for i in range(len(data) // 12):
    entry = data[i*12:(i+1)*12]
    if entry != b'\xff'*12 and entry != b'\x00'*12:
        can_id = 0x0D000000 | (kind << 16) | 0x0004
        dlc = DLC.get(kind, 8)
        payload = ' '.join(f'{b:02X}' for b in entry[0:dlc])
        print(f"  can0  {can_id:08X}   [{dlc}]  {payload}")
    kind = entry[10]
*/
