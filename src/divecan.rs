#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiveCanId {
    pub src: u8,
    pub dst: u8,
    pub kind: u8,
}

impl DiveCanId {
    const DIVECAN_PREFIX: u32 = 0x0D00_0000;

    pub fn new(src: u8, dst: u8, kind: u8) -> Self {
        Self { src, dst, kind }
    }
    pub fn from_u32(id: u32) -> Self {
        Self::new(
            (id & 0xFF) as u8,
            ((id >> 8) & 0xFF) as u8,
            ((id >> 16) & 0xFF) as u8,
        )
    }

    pub fn to_u32(&self) -> u32 {
        Self::DIVECAN_PREFIX
            | ((self.kind as u32) << 16)
            | ((self.dst as u32) << 8)
            | (self.src as u32)
    }

    pub fn reply(&self, kind: u8) -> Self {
        Self::new(self.dst, self.src, kind)
    }
}

impl From<u32> for DiveCanId {
    fn from(id: u32) -> Self {
        Self::from_u32(id)
    }
}

impl From<DiveCanId> for u32 {
    fn from(id: DiveCanId) -> u32 {
        id.to_u32()
    }
}

pub struct DiveCanFrame {
    kind: u8,
    dlc: u8,
    data: [u8; 8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    InvalidDlc(u8),
}

impl DiveCanFrame {
    pub fn new(kind: u8, dlc: u8, data: [u8; 8]) -> Result<Self, FrameError> {
        if dlc > 8 {
            return Err(FrameError::InvalidDlc(dlc));
        }
        Ok(Self { kind, dlc, data })
    }

    pub fn dlc(&self) -> u8 {
        self.dlc
    }

    pub fn kind(&self) -> u8 {
        self.kind
    }

    pub fn bytes(&self) -> &[u8] {
        &self.data[..self.dlc as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellsActive(u8);

impl CellsActive {
    pub fn new(cells: [bool; 3]) -> Self {
        Self((cells[0] as u8) | ((cells[1] as u8) << 1) | ((cells[2] as u8) << 2))
    }

    pub fn as_array(&self) -> [bool; 3] {
        [(self.0 & 1) != 0, (self.0 & 2) != 0, (self.0 & 4) != 0]
    }

    pub fn to_u8(&self) -> u8 {
        self.0
    }

    pub fn from_u8(v: u8) -> Self {
        Self(v & 0b111) // Only use lower 3 bits
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consensus {
    NotCalibrated,
    NoActiveCells,
    PpO2(PpO2Deci),
}

impl Consensus {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0xff => Self::NotCalibrated,
            0xfe => Self::NoActiveCells,
            other => Self::PpO2(other.into()),
        }
    }

    pub fn to_u8(self) -> u8 {
        match self {
            Self::NotCalibrated => 0xff,
            Self::NoActiveCells => 0xfe,
            Self::PpO2(v) => v.raw(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalStatusCode {
    Success,
    Ack,
    Rejected,
    LowExtBatt,
    SolenoidError,
    Fo2RangeError,
    PressureError,
    Unknown(u8),
}

impl CalStatusCode {
    pub fn from_byte(b: u8) -> Self {
        match b {
            0x01 => CalStatusCode::Success,
            0x05 => CalStatusCode::Ack,
            0x08 => CalStatusCode::Rejected,
            0x10 => CalStatusCode::LowExtBatt,
            0x18 => CalStatusCode::SolenoidError,
            0x20 => CalStatusCode::Fo2RangeError,
            0x28 => CalStatusCode::PressureError,
            other => CalStatusCode::Unknown(other),
        }
    }

    pub fn to_byte(self) -> u8 {
        match self {
            CalStatusCode::Success => 0x01,
            CalStatusCode::Ack => 0x05,
            CalStatusCode::Rejected => 0x08,
            CalStatusCode::LowExtBatt => 0x10,
            CalStatusCode::SolenoidError => 0x18,
            CalStatusCode::Fo2RangeError => 0x20,
            CalStatusCode::PressureError => 0x28,
            CalStatusCode::Unknown(b) => b,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoltageAlert {
    UnderVoltage,
    // Will only be sent when solenoid is active
    Clear,
    // Battery voltage too high when firing solenoid
    OverVoltage,
}

impl VoltageAlert {
    fn from_2bit_opt(v: u8) -> Option<Self> {
        match v & 0b11 {
            0b00 => None,
            0b01 => Some(Self::UnderVoltage),
            0b10 => Some(Self::Clear),
            0b11 => Some(Self::OverVoltage),
            _ => unreachable!(),
        }
    }

    fn to_2bit_opt(v: Option<Self>) -> u8 {
        match v {
            None => 0b00,
            Some(Self::UnderVoltage) => 0b01,
            Some(Self::Clear) => 0b10,
            Some(Self::OverVoltage) => 0b11,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurrentAlert {
    // Drawing less than 70 while solenoid is active
    UnderCurrent,
    // Will only be set when solenoid is active
    Clear,
    // Drawing more than 10 after or 185 while firing
    OverCurrent,
}

impl CurrentAlert {
    fn from_2bit_opt(v: u8) -> Option<Self> {
        match v & 0b11 {
            0b00 => None,
            0b01 => Some(Self::UnderCurrent),
            0b10 => Some(Self::Clear),
            0b11 => Some(Self::OverCurrent),
            _ => unreachable!(),
        }
    }

    fn to_2bit_opt(v: Option<Self>) -> u8 {
        match v {
            None => 0b00,
            Some(Self::UnderCurrent) => 0b01,
            Some(Self::Clear) => 0b10,
            Some(Self::OverCurrent) => 0b11,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownReason {
    UserInitiated,
    Timeout,
    Unknown(u8),
}

impl ShutdownReason {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0x00 => ShutdownReason::UserInitiated,
            0x01 => ShutdownReason::Timeout,
            other => ShutdownReason::Unknown(other),
        }
    }

    pub fn to_u8(self) -> u8 {
        match self {
            ShutdownReason::UserInitiated => 0x00,
            ShutdownReason::Timeout => 0x01,
            ShutdownReason::Unknown(v) => v,
        }
    }
}

#[derive(Debug)]
pub enum DecodeError {
    UnknownKind { kind: u8 },
    DlcMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Alert {
    //TODO: in use, seen 1/2
    unknown: u8,
    pub code: u16,
    details: [u8; 5],
    // DLC - header (3)
    details_len: u8,
}
impl Alert {
    pub fn new(unknown: u8, code: u16, details: &[u8]) -> Result<Self, DecodeError> {
        let details_len = details.len();
        if details_len > 5 {
            return Err(DecodeError::DlcMismatch);
        }
        let mut details_array = [0u8; 5];
        details_array[..details_len].copy_from_slice(details);
        Ok(Alert {
            unknown,
            code,
            details: details_array,
            details_len: details_len as u8,
        })
    }

    pub fn details(&self) -> &[u8] {
        &self.details[..self.details_len as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Msg {
    Id {
        manufacturer: u8,
        unused: u8,
        version: u8,
    },

    DeviceName([u8; 8]),

    Alert(Alert),

    ShutdownInit(ShutdownReason),

    CellPpo2([PpO2Deci; 3]),

    OboeStatus {
        battery_ok: bool,
        battery_voltage: Decivolt,
        unknown1: u8,
        unknown2: u8,
        //TODO: in use, seen 0x30
        unknown3: u8,
    },

    AmbientPressure {
        surface: Millibar,
        current: Millibar,
        depth_comp: bool,
    },

    //UDS Diagnostic
    Uds([u8; 8]),

    TankPressure {
        cylinder_index: u8,
        pressure: Decibar,
    },

    Nop,

    CellVoltages {
        cell_voltages: [CentiMillivolt; 3],
        unused: u8,
    },

    Ppo2CalibrationResponse {
        status: CalStatusCode,
        cell_voltages: [Millivolt; 3],
        fo2: Fo2,
        pressure: Millibar,
        cells_active: CellsActive,
    },

    Ppo2CalibrationRequest {
        fo2: Fo2,
        pressure: Millibar,
    },

    Co2Enabled(bool),

    Co2 {
        unknown: u8,
        pco2: Millibar,
    },

    Co2CalibrationResponse {
        code: u8,
        pco2: Millibar,
    },

    Co2CalibrationRequest {
        pco2: Millibar,
    },
    /// Sent when in bus devices menu on handset
    Undocumented30 {
        raw: [u8; 3],
    },

    BusInit {
        unused: [u8; 3],
    },

    TempProbe {
        sensor_id: u8,
        temp: u16,
    },
    /// This is RMS, probably scrubber time related
    UndocumentedC3 {
        unknown1: u16,
        unknown2: u16,
        unknown3: u8,
        unknown4: u8,
    },

    TempProbeEnabled(bool),

    Setpoint(PpO2Deci),

    CellStatus {
        cells_active: CellsActive,
        consensus: Consensus,
    },

    SoloStatus {
        voltage: Decivolt,
        current: Milliamp,
        injection_duration: Millisecond,
        setpoint: PpO2Deci,
        consensus: Consensus,
        voltage_alert: Option<VoltageAlert>,
        current_alert: Option<CurrentAlert>,
    },

    Diving {
        status: u8,
        dive_number: u16,
        timestamp: u32,
    },

    Serial([u8; 8]),
}
use Msg::*;

use crate::units::{
    CentiMillivolt, Decibar, Decivolt, Fo2, Milliamp, Millibar, Millisecond, Millivolt, PpO2Deci,
};

impl Msg {
    pub fn kind(&self) -> u8 {
        match self {
            Id { .. } => 0x00,
            DeviceName(..) => 0x01,
            Alert(..) => 0x02,
            ShutdownInit(..) => 0x03,
            CellPpo2(..) => 0x04,
            OboeStatus { .. } => 0x07,
            AmbientPressure { .. } => 0x08,
            Uds(..) => 0x0A,
            TankPressure { .. } => 0x0B,
            Nop => 0x10,
            CellVoltages { .. } => 0x11,
            Ppo2CalibrationResponse { .. } => 0x12,
            Ppo2CalibrationRequest { .. } => 0x13,
            Co2Enabled(..) => 0x20,
            Co2 { .. } => 0x21,
            Co2CalibrationResponse { .. } => 0x22,
            Co2CalibrationRequest { .. } => 0x23,
            Undocumented30 { .. } => 0x30,
            BusInit { .. } => 0x37,
            TempProbe { .. } => 0xC1,
            UndocumentedC3 { .. } => 0xC3,
            TempProbeEnabled(..) => 0xC4,
            Setpoint(..) => 0xC9,
            CellStatus { .. } => 0xCA,
            SoloStatus { .. } => 0xCB,
            Diving { .. } => 0xCC,
            Serial(..) => 0xD2,
        }
    }

    fn dlc(&self) -> u8 {
        match self {
            Id { .. } => 3,
            DeviceName(..) => 8,
            Alert(alert) => (3 + alert.details_len) as u8,
            ShutdownInit(..) => 1,
            CellPpo2(..) => 4,
            CellVoltages { .. } => 7,
            CellStatus { .. } => 2,
            SoloStatus { .. } => 8,
            OboeStatus { .. } => 5,
            AmbientPressure { .. } => 5,
            TankPressure { .. } => 3,
            Co2Enabled(..) => 1,
            Co2 { .. } => 3,
            Co2CalibrationResponse { .. } => 3,
            Co2CalibrationRequest { .. } => 2,
            Ppo2CalibrationRequest { .. } => 3,
            Ppo2CalibrationResponse { .. } => 8,
            Setpoint(..) => 1,
            TempProbeEnabled(..) => 1,
            TempProbe { .. } => 3,
            UndocumentedC3 { .. } => 6,
            Diving { .. } => 7,
            BusInit { .. } => 3,
            Undocumented30 { .. } => 3,
            Nop => 8,
            Uds(..) => 8,
            Serial(..) => 8,
        }
    }

    pub fn dlc_min_size(kind: u8) -> Option<u8> {
        match kind {
            0x00 => Some(3),
            0x01 => Some(8),
            0x02 => Some(3),
            0x03 => Some(1),
            0x04 => Some(4),
            0x07 => Some(5),
            0x08 => Some(5),
            0x0A => Some(8),
            0x0B => Some(3),
            0x10 => Some(8),
            0x11 => Some(7),
            0x12 => Some(8),
            0x13 => Some(3),
            0x20 => Some(1),
            0x21 => Some(3),
            0x22 => Some(3),
            0x23 => Some(2),
            0x30 => Some(3),
            0x37 => Some(3),
            0xC1 => Some(3),
            0xC3 => Some(6),
            0xC4 => Some(1),
            0xC9 => Some(1),
            0xCA => Some(2),
            0xCB => Some(8),
            0xCC => Some(7),
            0xD2 => Some(8),
            _ => None,
        }
    }

    pub fn to_frame(&self) -> DiveCanFrame {
        let mut b = [0u8; 8];
        match self {
            Id {
                manufacturer,
                version: firmware,
                unused: unknown,
            } => {
                b[0] = *manufacturer;
                b[1] = *unknown;
                b[2] = *firmware;
            }
            DeviceName(name) => b.copy_from_slice(name),
            Alert(alert) => {
                let raw = alert.code.to_be_bytes();
                b[0] = alert.unknown;
                b[1] = raw[0];
                b[2] = raw[1];
                let len = alert.details_len as usize;
                b[3..(3 + len)].copy_from_slice(&alert.details[..len]);
            }
            ShutdownInit(cause) => {
                b[0] = cause.to_u8();
            }
            CellPpo2(cells) => {
                b[0] = 0x00;
                b[1] = cells[0].raw();
                b[2] = cells[1].raw();
                b[3] = cells[2].raw();
            }
            OboeStatus {
                battery_ok,
                battery_voltage,
                unknown1: b2,
                unknown2: b3,
                unknown3: b4,
            } => {
                b[0] = if *battery_ok { 1 } else { 0 };
                b[1] = *&battery_voltage.raw();
                b[2] = *b2;
                b[3] = *b3;
                b[4] = *b4;
            }
            AmbientPressure {
                surface,
                current,
                depth_comp,
            } => {
                let s = surface.raw().to_be_bytes();
                let c = current.raw().to_be_bytes();
                b[0] = s[0];
                b[1] = s[1];
                b[2] = c[0];
                b[3] = c[1];
                b[4] = if *depth_comp { 1 } else { 0 };
            }
            Uds(raw) => b.copy_from_slice(raw),
            TankPressure {
                cylinder_index: cylinder,
                pressure: pressure_decibar,
            } => {
                b[0] = *cylinder;
                let p = pressure_decibar.raw().to_be_bytes();
                b[1] = p[0];
                b[2] = p[1];
            }
            Nop => {}
            CellVoltages {
                cell_voltages,
                unused: unknown,
            } => {
                let c1 = cell_voltages[0].raw().to_be_bytes();
                let c2 = cell_voltages[1].raw().to_be_bytes();
                let c3 = cell_voltages[2].raw().to_be_bytes();
                b[0] = c1[0];
                b[1] = c1[1];
                b[2] = c2[0];
                b[3] = c2[1];
                b[4] = c3[0];
                b[5] = c3[1];
                b[6] = *unknown;
            }
            Ppo2CalibrationResponse {
                status,
                cell_voltages,
                fo2,
                pressure,
                cells_active,
            } => {
                b[0] = status.to_byte();
                b[1] = cell_voltages[0].raw();
                b[2] = cell_voltages[1].raw();
                b[3] = cell_voltages[2].raw();
                b[4] = *&fo2.raw();
                let p = pressure.raw().to_be_bytes();
                b[5] = p[0];
                b[6] = p[1];
                b[7] = cells_active.to_u8();
            }
            Ppo2CalibrationRequest { fo2, pressure } => {
                b[0] = *&fo2.raw();
                let p = pressure.raw().to_be_bytes();
                b[1] = p[0];
                b[2] = p[1];
            }
            Co2Enabled(enabled) => {
                b[0] = if *enabled { 1 } else { 0 };
            }
            Co2 { unknown, pco2 } => {
                b[0] = *unknown;
                let v = pco2.raw().to_be_bytes();
                b[1] = v[0];
                b[2] = v[1];
            }
            Co2CalibrationResponse { code, pco2 } => {
                b[0] = *code;
                let v = pco2.raw().to_be_bytes();
                b[1] = v[0];
                b[2] = v[1];
            }
            Co2CalibrationRequest { pco2 } => {
                let v = pco2.raw().to_be_bytes();
                b[0] = v[0];
                b[1] = v[1];
            }
            Undocumented30 { raw } => b[0..3].copy_from_slice(raw),
            BusInit { unused } => {
                b[0..3].copy_from_slice(unused);
            }
            TempProbe { sensor_id, temp } => {
                b[0] = *sensor_id;
                let tempb = temp.to_be_bytes();
                b[1] = tempb[0];
                b[2] = tempb[1];
            }
            UndocumentedC3 {
                unknown1,
                unknown2,
                unknown3,
                unknown4,
            } => {
                let u = unknown1.to_be_bytes();
                let v = unknown2.to_be_bytes();
                b[0] = u[0];
                b[1] = u[1];
                b[2] = v[0];
                b[3] = v[1];
                b[4] = *unknown3;
                b[5] = *unknown4;
            }
            TempProbeEnabled(enabled) => {
                b[0] = if *enabled { 1 } else { 0 };
            }
            Setpoint(setpoint) => {
                b[0] = *&setpoint.raw();
            }
            CellStatus {
                cells_active,
                consensus,
            } => {
                b[0] = cells_active.to_u8();
                b[1] = consensus.to_u8();
            }
            SoloStatus {
                voltage: battery_voltage,
                current: solenoid_reading,
                injection_duration: unknown,
                setpoint,
                consensus,
                voltage_alert: battery_status,
                current_alert: solenoid_status,
            } => {
                let u = solenoid_reading.raw().to_be_bytes();
                let v = unknown.raw().to_be_bytes();
                b[0] = *&battery_voltage.raw();
                b[1] = u[0];
                b[2] = u[1];
                b[3] = v[0];
                b[4] = v[1];
                b[5] = *&setpoint.raw();
                b[6] = consensus.to_u8();
                let bat2 = VoltageAlert::to_2bit_opt(*battery_status) & 0b11;
                let sol2 = (CurrentAlert::to_2bit_opt(*solenoid_status) & 0b11) << 2;
                b[7] = bat2 | sol2;
            }
            Diving {
                status,
                dive_number,
                timestamp,
            } => {
                b[0] = *status;
                let dn = dive_number.to_be_bytes();
                b[1] = dn[0];
                b[2] = dn[1];
                let ts = timestamp.to_be_bytes();
                b[3] = ts[0];
                b[4] = ts[1];
                b[5] = ts[2];
                b[6] = ts[3];
            }
            Serial(serial) => b.copy_from_slice(serial),
        };

        DiveCanFrame {
            kind: self.kind(),
            dlc: self.dlc(),
            data: b,
        }
    }

    pub fn try_from_frame(frame: &DiveCanFrame) -> Result<Self, DecodeError> {
        match Self::dlc_min_size(frame.kind) {
            None => {
                return Err(DecodeError::UnknownKind { kind: frame.kind });
            }
            Some(expected) => {
                if frame.dlc < expected || frame.dlc > 8 {
                    return Err(DecodeError::DlcMismatch);
                }
            }
        }

        let data = frame.data;
        match frame.kind {
            0x00 => Ok(Id {
                manufacturer: data[0],
                unused: data[1],
                version: data[2],
            }),
            0x01 => Ok(DeviceName(data)),
            0x02 => {
                let len = (frame.dlc as usize).saturating_sub(3);
                let code = u16::from_be_bytes([data[1], data[2]]);
                let details = &data[3..3 + len];
                let alert = crate::divecan::Alert::new(data[0], code, details)?;
                Ok(Alert(alert))
            }
            0x03 => Ok(ShutdownInit(ShutdownReason::from_u8(data[0]))),
            0x04 => Ok(CellPpo2([data[1].into(), data[2].into(), data[3].into()])),
            0x07 => Ok(OboeStatus {
                battery_ok: data[0] != 0,
                battery_voltage: data[1].into(),
                unknown1: data[2],
                unknown2: data[3],
                unknown3: data[4],
            }),
            0x08 => Ok(AmbientPressure {
                surface: u16::from_be_bytes([data[0], data[1]]).into(),
                current: u16::from_be_bytes([data[2], data[3]]).into(),
                depth_comp: data[4] != 0,
            }),
            0x0A => Ok(Uds(data)),
            0x0B => Ok(TankPressure {
                cylinder_index: data[0],
                pressure: u16::from_be_bytes([data[1], data[2]]).into(),
            }),
            0x10 => Ok(Nop),
            0x11 => Ok(CellVoltages {
                cell_voltages: [
                    u16::from_be_bytes([data[0], data[1]]).into(),
                    u16::from_be_bytes([data[2], data[3]]).into(),
                    u16::from_be_bytes([data[4], data[5]]).into(),
                ],
                unused: data[6],
            }),
            0x12 => Ok(Ppo2CalibrationResponse {
                status: CalStatusCode::from_byte(data[0]),
                cell_voltages: [data[1].into(), data[2].into(), data[3].into()],
                fo2: data[4].into(),
                pressure: u16::from_be_bytes([data[5], data[6]]).into(),
                cells_active: CellsActive::from_u8(data[7]),
            }),
            0x13 => Ok(Ppo2CalibrationRequest {
                fo2: data[0].into(),
                pressure: u16::from_be_bytes([data[1], data[2]]).into(),
            }),
            0x20 => Ok(Co2Enabled(data[0] != 0)),
            0x21 => Ok(Co2 {
                unknown: data[0],
                pco2: u16::from_be_bytes([data[1], data[2]]).into(),
            }),
            0x22 => Ok(Co2CalibrationResponse {
                code: data[0],
                pco2: u16::from_be_bytes([data[1], data[2]]).into(),
            }),
            0x23 => Ok(Co2CalibrationRequest {
                pco2: u16::from_be_bytes([data[0], data[1]]).into(),
            }),
            0x30 => Ok(Undocumented30 {
                raw: data[0..3].try_into().unwrap(),
            }),
            0x37 => Ok(BusInit {
                unused: data[0..3].try_into().unwrap(),
            }),
            0xC1 => Ok(TempProbe {
                sensor_id: data[0],
                temp: u16::from_be_bytes([data[1], data[2]]),
            }),
            0xC3 => Ok(UndocumentedC3 {
                unknown1: u16::from_be_bytes([data[0], data[1]]),
                unknown2: u16::from_be_bytes([data[2], data[3]]),
                unknown3: data[4],
                unknown4: data[5],
            }),
            0xC4 => Ok(TempProbeEnabled(data[0] != 0)),
            0xC9 => Ok(Setpoint(data[0].into())),
            0xCA => Ok(CellStatus {
                cells_active: CellsActive::from_u8(data[0]),
                consensus: Consensus::from_u8(data[1]),
            }),

            0xCB => Ok(Msg::SoloStatus {
                voltage: data[0].into(),
                current: u16::from_be_bytes([data[1], data[2]]).into(),
                injection_duration: u16::from_be_bytes([data[3], data[4]]).into(),
                setpoint: data[5].into(),
                consensus: Consensus::from_u8(data[6]),
                voltage_alert: VoltageAlert::from_2bit_opt(data[7] & 0b0011),
                current_alert: CurrentAlert::from_2bit_opt((data[7] & 0b1100) >> 2),
            }),
            0xCC => Ok(Diving {
                status: data[0],
                dive_number: u16::from_be_bytes([data[1], data[2]]),
                timestamp: u32::from_be_bytes([data[3], data[4], data[5], data[6]]),
            }),
            0xD2 => Ok(Serial(data)),
            other => return Err(DecodeError::UnknownKind { kind: other }),
        }
    }
}

// Standard trait implementations for idiomatic conversion
impl From<Msg> for DiveCanFrame {
    fn from(msg: Msg) -> Self {
        msg.to_frame()
    }
}

impl From<&Msg> for DiveCanFrame {
    fn from(msg: &Msg) -> Self {
        msg.to_frame()
    }
}

impl TryFrom<&DiveCanFrame> for Msg {
    type Error = DecodeError;

    fn try_from(frame: &DiveCanFrame) -> Result<Self, Self::Error> {
        Msg::try_from_frame(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_msg_size() {
        use core::mem::size_of;
        println!("Msg size: {} bytes", size_of::<Msg>());
        println!("Alert size: {} bytes", size_of::<Alert>());

        // Check specific variants
        let id = Msg::Id {
            manufacturer: 0,
            unused: 0,
            version: 0,
        };
        let uds = Msg::Uds([0; 8]);
        let device_name = Msg::DeviceName([0; 8]);

        // Msg should be Copy-able if it's small enough
        println!("Is Msg Copy? {}", core::mem::needs_drop::<Msg>());
    }

    #[test]
    fn dlc_matches_encoded_payload_length() {
        use super::*;

        fn assert_zero_tail(m: &Msg) {
            let dlc = m.dlc() as usize;
            assert!(dlc <= 8, "DLC out of range for {:?}: dlc={}", m, dlc);

            let bytes = m.to_frame().data;
            let tail = &bytes[dlc..];
            assert!(
                tail.iter().all(|&b| b == 0),
                "Non-zero bytes past DLC for {:?}: dlc={}, bytes={:02X?}",
                m,
                dlc,
                bytes
            );
        }

        // Use non-zero values so mistakes are visible.
        let cases: Vec<Msg> = vec![
            Msg::Id {
                manufacturer: 0xAA,
                unused: 0xBB,
                version: 0xCC,
            },
            Msg::DeviceName(*b"YOLO    "),
            Msg::Alert(Alert::new(0xff, 0x1234, &[0xEE; 5]).unwrap()),
            Msg::ShutdownInit(ShutdownReason::Timeout),
            Msg::CellPpo2([0x11.into(), 0x22.into(), 0x33.into()]),
            Msg::OboeStatus {
                battery_ok: true,
                battery_voltage: 0x64.into(),
                unknown1: 0x01,
                unknown2: 0x02,
                unknown3: 0x03,
            },
            Msg::AmbientPressure {
                surface: 1013.into(),
                current: 1245.into(),
                depth_comp: true,
            },
            Msg::Uds([1, 2, 3, 4, 5, 6, 7, 8]),
            Msg::TankPressure {
                cylinder_index: 0x02,
                pressure: 0x1234.into(),
            },
            Msg::Nop,
            Msg::CellVoltages {
                cell_voltages: [3200.into(), 3300.into(), 3400.into()],
                unused: 0x7F,
            },
            Msg::Ppo2CalibrationRequest {
                fo2: 0x21.into(),
                pressure: 1000.into(),
            },
            Msg::Ppo2CalibrationResponse {
                status: CalStatusCode::Ack,
                cell_voltages: [0x10.into(), 0x20.into(), 0x30.into()],
                fo2: 0x21.into(),
                pressure: 1000.into(),
                cells_active: CellsActive::new([true, true, true]),
            },
            Msg::Co2Enabled(true),
            Msg::Co2 {
                unknown: 0x55,
                pco2: 1234.into(),
            },
            Msg::Co2CalibrationResponse {
                code: 0x66,
                pco2: 2222.into(),
            },
            Msg::Co2CalibrationRequest { pco2: 3333.into() },
            Msg::Undocumented30 { raw: [9, 8, 7] },
            Msg::BusInit {
                unused: [0xA1, 0xA2, 0xA3],
            },
            Msg::TempProbe {
                sensor_id: 0x02,
                temp: 0x0BEE,
            },
            Msg::UndocumentedC3 {
                unknown1: 0xFFFF,
                unknown2: 0xFFFF,
                unknown3: 0xFF,
                unknown4: 0xFF,
            },
            Msg::TempProbeEnabled(true),
            Msg::Setpoint(0x42.into()),
            Msg::CellStatus {
                cells_active: CellsActive::new([true, true, true]),
                consensus: Consensus::PpO2(0x55.into()),
            },
            Msg::SoloStatus {
                voltage: 0x64.into(),
                current: 0xff.into(),
                injection_duration: 0xff.into(),
                setpoint: 0x03.into(),
                consensus: Consensus::PpO2(0x09.into()),
                voltage_alert: None,
                current_alert: None,
            },
            Msg::Diving {
                status: 0x01,
                dive_number: 0x1234,
                timestamp: 1_700_000_000,
            },
            Msg::Serial([0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80]),
        ];

        for m in cases {
            assert_zero_tail(&m);
        }
    }

    #[test]
    fn roundtrip_id() {
        let m = Msg::Id {
            manufacturer: 1,
            version: 0x84,
            unused: 0,
        };
        let m2 = Msg::try_from_frame(&m.to_frame()).unwrap();
        assert_eq!(m, m2);
    }

    #[test]
    fn roundtrip_alert() {
        let m = Msg::Alert(Alert::new(0xff, 0x1234, &[0xff; 5]).unwrap());
        let m2 = Msg::try_from_frame(&m.to_frame()).unwrap();
        assert_eq!(m, m2);
    }

    #[test]
    fn roundtrip_diving() {
        let m = Msg::Diving {
            status: 0x00,
            dive_number: 42,
            timestamp: 1_700_000_000,
        };
        let m2 = Msg::try_from_frame(&m.to_frame()).unwrap();
        assert_eq!(m, m2);
    }

    #[test]
    fn test_from_trait() {
        let msg = Msg::Id {
            manufacturer: 1,
            unused: 0,
            version: 0x42,
        };

        let frame: DiveCanFrame = msg.into();
        assert_eq!(frame.kind(), 0x00);

        let frame2: DiveCanFrame = (&msg).into();
        assert_eq!(frame2.kind(), 0x00);

        assert_eq!(msg.kind(), 0x00);
    }

    #[test]
    fn test_tryfrom_trait() {
        let msg = Msg::Setpoint(0x70.into());
        let frame = msg.to_frame();

        // Test TryFrom<&DiveCanFrame>
        let msg2: Msg = (&frame).try_into().unwrap();
        assert_eq!(msg, msg2);
    }

    #[test]
    fn rejects_wrong_dlc() {
        let mut f = Msg::Diving {
            status: 0,
            dive_number: 1,
            timestamp: 2,
        }
        .to_frame();
        f.dlc = 5; // expected minimum 7
        assert!(matches!(
            Msg::try_from_frame(&f),
            Err(DecodeError::DlcMismatch)
        ));
    }
}
