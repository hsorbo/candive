use candive::{alerts::*, divecan::*};

pub fn pretty(msg: &Msg) -> String {
    fn ascii_lossy(bytes: &[u8]) -> String {
        bytes
            .iter()
            .map(|&b| match b {
                0x20..=0x7E => b as char,
                _ => '.',
            })
            .collect()
    }

    fn consensus_text(c: Consensus) -> String {
        match c {
            Consensus::NotCalibrated => "not calibrated".into(),
            Consensus::NoActiveCells => "no active cells".into(),
            Consensus::PpO2(v) => format!("{v}"),
        }
    }

    fn voltage_alert_text(v: Option<VoltageAlert>) -> &'static str {
        match v {
            None => "",
            Some(VoltageAlert::UnderVoltage) => "battery undervoltage",
            Some(VoltageAlert::Clear) => "battery: clear",
            Some(VoltageAlert::OverVoltage) => "battery: overvoltage",
        }
    }

    fn current_alert_text(v: Option<CurrentAlert>) -> &'static str {
        match v {
            None => "",
            Some(CurrentAlert::UnderCurrent) => "solenoid: undercurrent",
            Some(CurrentAlert::Clear) => "solenoid: clear",
            Some(CurrentAlert::OverCurrent) => "solenoid: overcurrent",
        }
    }

    fn alert_label(code: u16) -> String {
        if let Some(a) = HandsetAlert::from_u16(code) {
            return match a {
                HandsetAlert::ShutdownWhileBluetooth => "shutdown while Bluetooth active".into(),
                HandsetAlert::ShutdownWhileDiving => "shutdown while diving".into(),
                HandsetAlert::ShutdownWhileFwUpgrade => "shutdown during firmware upgrade".into(),
                HandsetAlert::ShutdownWhileUnknown => "shutdown for unknown reason".into(),
            };
        }
        if let Some(a) = TempAlert::from_u16(code) {
            return match a {
                TempAlert::TempProbeFailed => "temperature probe failure".into(),
            };
        }
        if let Some(a) = SoloAlert::from_u16(code) {
            return match a {
                SoloAlert::SoloCellStatusMaskZero => "no active oxygen cells".into(),
                SoloAlert::SoloSetpointTimeout => "setpoint timeout".into(),
                SoloAlert::SoloSetpointUpdateTimeout => "setpoint update timeout".into(),
                SoloAlert::SoloPPO2Below004PPO2 => "ppO₂ below 0.04".into(),
                SoloAlert::SoloSetpointOutOfRange => "setpoint out of range".into(),

                SoloAlert::SoloFwCrcFailed => "firmware CRC check failed".into(),
                SoloAlert::SoloFwCrcReset => "reset due to firmware CRC error".into(),
                SoloAlert::SoloReadSettingsFailed => "failed to read settings".into(),
                SoloAlert::SoloSpiFlashBusy => "SPI flash busy".into(),

                SoloAlert::IsotpSingleFrameSendFailed => "ISO-TP single-frame send failed".into(),
                SoloAlert::IsotpFlowControlTimeout => "ISO-TP flow-control timeout".into(),
                SoloAlert::IsotpBusySingleFrame => "ISO-TP busy (single frame)".into(),
                SoloAlert::IsotpBusyFirstFrame => "ISO-TP busy (first frame)".into(),

                SoloAlert::UdsTransferDownloadOutOfRange => "UDS download out of range".into(),
                SoloAlert::UdsTransferDownloadProgFailed => {
                    "UDS download programming failed".into()
                }
                SoloAlert::UdsTransferIncorrectMessageLength => {
                    "UDS incorrect message length".into()
                }
                SoloAlert::UdsTransferDownloadWrongSequence => "UDS wrong download sequence".into(),
                SoloAlert::UdsTransferWrongBlockSequence => "UDS wrong block sequence".into(),
                SoloAlert::UdsTransferRequestSequenceError => "UDS request sequence error".into(),
                SoloAlert::UdsTransferExitFailed => "UDS transfer exit failed".into(),
                SoloAlert::UdsTransferNoBlocksTransferred => "UDS no blocks transferred".into(),
                SoloAlert::UdsTransferCrcVerifyFailed => "UDS CRC verify failed".into(),
                SoloAlert::UdsTransferCrcMismatch => "UDS CRC mismatch".into(),
                SoloAlert::UdsTransferVerifyProgFailed => "UDS verify programming failed".into(),
                SoloAlert::UdsTransferUploadFailed => "UDS upload failed".into(),
                SoloAlert::UdsTransferTimeout => "UDS transfer timeout".into(),
            };
        }

        format!("unknown alert 0x{code:04X}")
    }

    match msg {
        Msg::Id {
            manufacturer,
            version,
            ..
        } => format!("device ID: manufacturer 0x{manufacturer:02X}, firmware 0x{version:02X}"),

        Msg::DeviceName(name) => format!("device name: \"{}\"", ascii_lossy(name)),

        Msg::Alert(alert) => format!("alert: {}", alert_label(alert.code)),

        Msg::ShutdownInit(reason) => format!("shutdown initiated: {reason:?}"),

        Msg::CellPpo2(cells) => format!(
            "cell ppO₂ readings: {}, {}, {}",
            cells[0], cells[1], cells[2]
        ),

        Msg::OboeStatus {
            battery_ok,
            battery_voltage,
            ..
        } => format!(
            "battery status: {}, voltage {}",
            if *battery_ok { "OK" } else { "not OK" },
            battery_voltage
        ),

        Msg::AmbientPressure {
            surface,
            current,
            depth_comp,
        } => format!(
            "ambient pressure: surface {}, current {}, depth compensation {}",
            surface,
            current,
            if *depth_comp { "enabled" } else { "disabled" }
        ),

        Msg::Uds { dlc, data } => format!(
            "UDS diagnostic data: {} bytes [{}]",
            dlc,
            data[..(*dlc as usize).min(8)]
                .iter()
                .map(|b| format!("{:02X}", b))
                .collect::<Vec<_>>()
                .join(" ")
        ),

        Msg::TankPressure {
            cylinder_index,
            pressure,
        } => format!("tank pressure: cylinder {}, {}", cylinder_index, pressure),

        Msg::Nop => "no operation".into(),

        Msg::CellVoltages { cell_voltages, .. } => format!(
            "cell voltages: {}, {}, {}",
            cell_voltages[0], cell_voltages[1], cell_voltages[2]
        ),

        Msg::Ppo2CalibrationRequest { fo2, pressure } => {
            format!(
                "ppO₂ calibration requested: FO₂ {}, pressure {}",
                fo2, pressure
            )
        }

        Msg::Ppo2CalibrationResponse {
            status,
            cell_voltages,
            fo2,
            pressure,
            cells_active,
        } => format!(
            "ppO₂ calibration result: {:?}, cells {}, {}, {}, FO₂ {}, pressure {}, active cells {:?}",
            status,
            cell_voltages[0],
            cell_voltages[1],
            cell_voltages[2],
            fo2,
            pressure,
            cells_active.as_array()
        ),

        Msg::Co2Enabled(enabled) => format!(
            "CO₂ monitoring {}",
            if *enabled { "enabled" } else { "disabled" }
        ),

        Msg::Co2 { pco2, .. } => format!("CO₂ partial pressure {}", pco2),
        Msg::Co2CalibrationRequest { pco2 } => format!("CO₂ calibration requested at {}", pco2),
        Msg::Co2CalibrationResponse { pco2, .. } => format!("CO₂ calibration result {}", pco2),

        Msg::TempProbe { sensor_id, temp } => {
            format!("temperature probe {} reading {}", sensor_id, temp)
        }

        Msg::TempProbeEnabled(enabled) => format!(
            "temperature probe {}",
            if *enabled { "enabled" } else { "disabled" }
        ),

        Msg::Setpoint(sp) => format!("setpoint changed to {}", sp),

        Msg::CellStatus {
            cells_active,
            consensus,
        } => format!(
            "cell status: active cells {:?}, consensus {}",
            cells_active.as_array(),
            consensus_text(*consensus)
        ),

        Msg::SoloStatus {
            voltage,
            current,
            injection_duration,
            setpoint,
            consensus,
            voltage_alert,
            current_alert,
        } => format!(
            "solo status: voltage {}, current {}, injection {}, setpoint {}, consensus {}, {}, {}",
            voltage,
            current,
            injection_duration,
            setpoint,
            consensus_text(*consensus),
            voltage_alert_text(*voltage_alert),
            current_alert_text(*current_alert),
        ),

        Msg::Diving {
            status,
            dive_number,
            timestamp,
        } => format!(
            "diving state: status 0x{status:02X}, dive #{dive_number}, timestamp {timestamp}"
        ),

        Msg::Serial(bytes) => format!("serial number: \"{}\"", ascii_lossy(bytes)),

        Msg::Undocumented30 { .. } => "undocumented message (0x30)".into(),

        Msg::UndocumentedC3 { .. } => "undocumented message (0xC3)".into(),

        Msg::BusInit { .. } => "bus initialization".into(),
    }
}
