#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DidDecodeError {
    TooShort { needed: usize },
    BadLength { expected: usize },
    InvalidFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DidAccess {
    /// Read-only (RDBI only)
    ReadOnly,
    /// Write-only (WDBI only)
    WriteOnly,
    /// Read and write (RDBI and WDBI)
    ReadWrite,
}

pub trait DataIdentifier: for<'a> TryFrom<&'a [u8], Error = DidDecodeError> {
    const DID: u16;
    const ACCESS: DidAccess;
    type Bytes: AsRef<[u8]> + Copy;

    fn to_bytes(&self) -> Self::Bytes;
}

macro_rules! define_byte_array_did {
    (
        $(#[$meta:meta])*
        $name:ident,
        did: $did:expr,
        access: $access:expr,
        len: $len:expr,
        field: $field:ident
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct $name {
            pub $field: [u8; $len],
        }

        impl DataIdentifier for $name {
            const DID: u16 = $did;
            const ACCESS: DidAccess = $access;
            type Bytes = [u8; $len];

            fn to_bytes(&self) -> Self::Bytes {
                self.$field
            }
        }

        impl TryFrom<&[u8]> for $name {
            type Error = DidDecodeError;

            fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
                if bytes.len() != $len {
                    return Err(DidDecodeError::BadLength { expected: $len });
                }
                let mut $field = [0u8; $len];
                $field.copy_from_slice(bytes);
                Ok($name { $field })
            }
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PPO2ControlMode {
    UserSelect = 0,
    Manual = 1,
    OneSec = 2,
    FiveSec = 3,
}

impl PPO2ControlMode {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::UserSelect),
            1 => Some(Self::Manual),
            2 => Some(Self::OneSec),
            3 => Some(Self::FiveSec),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CalibrationProcedure {
    /// Direct calibration - immediately reads and calibrates
    Direct = 0,
    /// Monitored calibration - verifies stability, solenoid, and battery before calibrating
    Monitored = 1,
}

impl CalibrationProcedure {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Direct),
            1 => Some(Self::Monitored),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellMode {
    //Uses 2 oxygen sensors, PID gains Kp=1.2, max pulse 9ms
    TwoCell,
    //Uses 3 oxygen sensors, PID gains Kp=2.5, max pulse 14ms
    ThreeCell,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoloConfigDid {
    pub calibration_procedure: CalibrationProcedure,
    pub ppo2_control_mode: PPO2ControlMode,
    pub cell_mode: CellMode,
    pub depth_compensation_enabled: bool,
    pub solenoid_current_min_ma: u16,
    /// Solenoid maximum current in mA
    pub solenoid_current_max_ma: u16,
    /// Battery minimum voltage
    pub battery_voltage_min: u16,
    pub battery_voltage_doubling: bool,
    pub reserved_bits_20_21: u8,
    pub reserved_bits_24_31: u8,
}

impl DataIdentifier for SoloConfigDid {
    const DID: u16 = 0x820b;
    const ACCESS: DidAccess = DidAccess::ReadOnly;
    type Bytes = [u8; 4];

    fn to_bytes(&self) -> Self::Bytes {
        let mut config_word: u32 = 0;

        if self.calibration_procedure == CalibrationProcedure::Monitored {
            config_word |= 1;
        } else {
            config_word |= 2;
        }

        config_word |= (self.ppo2_control_mode as u32 & 0x3) << 2;

        if self.cell_mode == CellMode::ThreeCell {
            config_word |= 1 << 4;
        }

        if self.depth_compensation_enabled {
            config_word |= 1 << 6;
        } else {
            config_word |= 2 << 6;
        }

        let sol_min_raw = (self.solenoid_current_min_ma.saturating_sub(50) / 10) & 0xF;
        config_word |= (sol_min_raw as u32) << 8;

        let sol_max_raw = (self.solenoid_current_max_ma.saturating_sub(50) / 10) & 0x1F;
        config_word |= ((sol_max_raw & 0xF) as u32) << 12;
        config_word |= ((sol_max_raw >> 4) as u32) << 23;

        let mut batt_raw = self.battery_voltage_min;
        if self.battery_voltage_doubling {
            batt_raw >>= 1;
        }
        batt_raw = (batt_raw.saturating_sub(50) >> 1) & 0xF;
        config_word |= (batt_raw as u32) << 16;

        if self.battery_voltage_doubling {
            config_word |= 1 << 22;
        }

        config_word |= (self.reserved_bits_20_21 as u32 & 0x3) << 20;
        config_word |= (self.reserved_bits_24_31 as u32) << 24;

        config_word.to_be_bytes()
    }
}

impl TryFrom<&[u8]> for SoloConfigDid {
    type Error = DidDecodeError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let arr: [u8; 4] = bytes
            .try_into()
            .map_err(|_| DidDecodeError::BadLength { expected: 4 })?;
        let config_word = u32::from_be_bytes(arr);

        let calibration_procedure = if (config_word & 0x3) == 1 {
            CalibrationProcedure::Monitored
        } else {
            CalibrationProcedure::Direct
        };
        let ppo2_control_mode = PPO2ControlMode::from_u8(((config_word >> 2) & 0x3) as u8)
            .ok_or(DidDecodeError::InvalidFormat)?;
        let cell_averaging_3cell = ((config_word >> 4) & 0x3) == 1;
        let depth_compensation_enabled = ((config_word >> 6) & 0x3) == 1;

        let sol_min_raw = ((config_word >> 8) & 0xF) as u16;
        let solenoid_current_min_ma = 50 + (sol_min_raw * 10);

        let sol_max_lo = ((config_word >> 12) & 0xF) as u16;
        let sol_max_hi = ((config_word >> 23) & 0x1) as u16;
        let solenoid_current_max_ma = 50 + (((sol_max_hi << 4) | sol_max_lo) * 10);

        let batt_min_raw = ((config_word >> 16) & 0xF) as u16;
        let mut battery_voltage_min = 50 + (batt_min_raw << 1);

        let battery_voltage_doubling = ((config_word >> 22) & 0x1) == 1;
        if battery_voltage_doubling {
            battery_voltage_min <<= 1;
        }

        let cell_mode = if cell_averaging_3cell {
            CellMode::ThreeCell
        } else {
            CellMode::TwoCell
        };

        let reserved_bits_20_21 = ((config_word >> 20) & 0x3) as u8;
        let reserved_bits_24_31 = ((config_word >> 24) & 0xFF) as u8;

        Ok(SoloConfigDid {
            calibration_procedure,
            ppo2_control_mode,
            cell_mode,
            depth_compensation_enabled,
            solenoid_current_min_ma,
            solenoid_current_max_ma,
            battery_voltage_min,
            battery_voltage_doubling,
            reserved_bits_20_21,
            reserved_bits_24_31,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwareDownloadInfoDid {
    pub supported: bool,
    pub address: u32,
    pub max_size: u32,
}

impl DataIdentifier for FirmwareDownloadInfoDid {
    const DID: u16 = 0x8020;
    const ACCESS: DidAccess = DidAccess::ReadOnly;
    type Bytes = [u8; 9];

    fn to_bytes(&self) -> Self::Bytes {
        let mut result = [0u8; 9];
        result[0] = if self.supported { 1 } else { 0 };
        result[1..5].copy_from_slice(&self.address.to_be_bytes());
        result[5..9].copy_from_slice(&self.max_size.to_be_bytes());
        result
    }
}

impl TryFrom<&[u8]> for FirmwareDownloadInfoDid {
    type Error = DidDecodeError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.len() != 9 {
            return Err(DidDecodeError::BadLength { expected: 9 });
        }

        let supported = bytes[0] != 0;
        let address = u32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
        let max_size = u32::from_be_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]);

        Ok(FirmwareDownloadInfoDid {
            supported,
            address,
            max_size,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogUploadInfoDid {
    pub supported: bool,
    pub address: u32,
    pub size: u32,
}

impl DataIdentifier for LogUploadInfoDid {
    const DID: u16 = 0x8021;
    const ACCESS: DidAccess = DidAccess::ReadOnly;
    type Bytes = [u8; 9];

    fn to_bytes(&self) -> Self::Bytes {
        let mut result = [0u8; 9];
        result[0] = if self.supported { 1 } else { 0 };
        result[1..5].copy_from_slice(&self.address.to_be_bytes());
        result[5..9].copy_from_slice(&self.size.to_be_bytes());
        result
    }
}

impl TryFrom<&[u8]> for LogUploadInfoDid {
    type Error = DidDecodeError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.len() != 9 {
            return Err(DidDecodeError::BadLength { expected: 9 });
        }

        let supported = bytes[0] != 0;
        let address = u32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
        let size = u32::from_be_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]);

        Ok(LogUploadInfoDid {
            supported,
            address,
            size,
        })
    }
}

/// O2 cell runtime calibration data (DID 0x8203)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoloO2CellCalibrationDid {
    pub o2_calibrations: [u32; 3],
    pub calibration_valid: [bool; 3],
}

impl DataIdentifier for SoloO2CellCalibrationDid {
    const DID: u16 = 0x8203;
    const ACCESS: DidAccess = DidAccess::ReadOnly;
    type Bytes = [u8; 15];

    fn to_bytes(&self) -> Self::Bytes {
        let mut result = [0u8; 15];
        result[0..4].copy_from_slice(&self.o2_calibrations[0].to_be_bytes());
        result[4..8].copy_from_slice(&self.o2_calibrations[1].to_be_bytes());
        result[8..12].copy_from_slice(&self.o2_calibrations[2].to_be_bytes());
        result[12] = if self.calibration_valid[0] { 1 } else { 0 };
        result[13] = if self.calibration_valid[1] { 1 } else { 0 };
        result[14] = if self.calibration_valid[2] { 1 } else { 0 };
        result
    }
}

impl TryFrom<&[u8]> for SoloO2CellCalibrationDid {
    type Error = DidDecodeError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let arr: [u8; 15] = bytes
            .try_into()
            .map_err(|_| DidDecodeError::BadLength { expected: 15 })?;
        
        let mut calibration_valid = [false; 3];
        for i in 0..3 {
            calibration_valid[i] = match arr[12 + i] {
                0 => false,
                1 => true,
                _ => return Err(DidDecodeError::InvalidFormat),
            };
        }
        
        Ok(Self {
            o2_calibrations: [
                u32::from_be_bytes([arr[0], arr[1], arr[2], arr[3]]),
                u32::from_be_bytes([arr[4], arr[5], arr[6], arr[7]]),
                u32::from_be_bytes([arr[8], arr[9], arr[10], arr[11]]),
            ],
            calibration_valid,
        })
    }
}

/// ADC voltage reference calibration value
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoloAdcVrefCalibrationDid(pub u32);

impl SoloAdcVrefCalibrationDid {
    pub const MIN: u32 = 0xa64;
    pub const MAX: u32 = 0xb7c;

    pub fn new(value: u32) -> Self {
        Self(value)
    }

    pub fn is_valid(&self) -> bool {
        self.0 >= Self::MIN && self.0 <= Self::MAX
    }
}

impl DataIdentifier for SoloAdcVrefCalibrationDid {
    const DID: u16 = 0x820a;
    const ACCESS: DidAccess = DidAccess::ReadWrite;
    type Bytes = [u8; 4];

    fn to_bytes(&self) -> Self::Bytes {
        self.0.to_be_bytes()
    }
}

impl TryFrom<&[u8]> for SoloAdcVrefCalibrationDid {
    type Error = DidDecodeError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let arr: [u8; 4] = bytes
            .try_into()
            .map_err(|_| DidDecodeError::BadLength { expected: 4 })?;
        let value = u32::from_be_bytes(arr);
        Ok(Self::new(value))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoloO2CellFactoryCalibrationDid {
    pub cells: [u32; 3],
}

impl DataIdentifier for SoloO2CellFactoryCalibrationDid {
    const DID: u16 = 0x8205;
    const ACCESS: DidAccess = DidAccess::ReadOnly;
    type Bytes = [u8; 12];

    fn to_bytes(&self) -> Self::Bytes {
        let mut result = [0u8; 12];
        result[0..4].copy_from_slice(&self.cells[0].to_be_bytes());
        result[4..8].copy_from_slice(&self.cells[1].to_be_bytes());
        result[8..12].copy_from_slice(&self.cells[2].to_be_bytes());
        result
    }
}

impl TryFrom<&[u8]> for SoloO2CellFactoryCalibrationDid {
    type Error = DidDecodeError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let arr: [u8; 12] = bytes
            .try_into()
            .map_err(|_| DidDecodeError::BadLength { expected: 12 })?;
        Ok(Self {
            cells: [
                u32::from_be_bytes([arr[0], arr[1], arr[2], arr[3]]),
                u32::from_be_bytes([arr[4], arr[5], arr[6], arr[7]]),
                u32::from_be_bytes([arr[8], arr[9], arr[10], arr[11]]),
            ],
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwareCrcDid {
    pub crc: u32,
}

impl DataIdentifier for FirmwareCrcDid {
    const DID: u16 = 0x8209;
    const ACCESS: DidAccess = DidAccess::ReadOnly;
    type Bytes = [u8; 4];

    fn to_bytes(&self) -> Self::Bytes {
        self.crc.to_be_bytes()
    }
}

impl TryFrom<&[u8]> for FirmwareCrcDid {
    type Error = DidDecodeError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let arr: [u8; 4] = bytes
            .try_into()
            .map_err(|_| DidDecodeError::BadLength { expected: 4 })?;
        let crc = u32::from_be_bytes(arr);
        Ok(FirmwareCrcDid { crc })
    }
}

define_byte_array_did!(
    SerialStringDid,
    did: 0x8010,
    access: DidAccess::ReadOnly,
    len: 8,
    field: serial_ascii
);

define_byte_array_did!(
    VersionStringDid,
    did: 0x8011,
    access: DidAccess::ReadOnly,
    len: 3,
    field: firmare_version_ascii
);

define_byte_array_did!(
    SerialDid,
    did: 0x8200,
    access: DidAccess::ReadWrite,
    len: 4,
    field: serial
);

define_byte_array_did!(
    // The actual DeviceId from CPU registers
    DeviceIdDid,
    did: 0x8201,
    access: DidAccess::ReadOnly,
    len: 12,
    field: device_id
);

define_byte_array_did!(
    // DeviceId from settings, not actual
    SoloEncryptedConfigAndIdDid,
    did: 0x8202,
    access: DidAccess::ReadWrite,
    len: 16,
    field: unknown
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_0x8010() {
        // 0x8010 -> 4130303544303037 = ASCII "A005D007"
        let input = hex::decode("4130303544303037").unwrap();
        let result = SerialStringDid::try_from(input.as_slice()).unwrap();
        assert_eq!(result.to_bytes(), result.serial_ascii);

        assert_eq!(&result.serial_ascii, b"A005D007");
    }

    #[test]
    fn test_0x8011() {
        // 0x8011 -> 763132 = ASCII "v12"
        let input = hex::decode("763132").unwrap();
        let result = VersionStringDid::try_from(input.as_slice()).unwrap();
        assert_eq!(&result.to_bytes()[..], &input[..]);

        assert_eq!(&result.firmare_version_ascii, b"v12");
    }

    #[test]
    fn test_0x8020() {
        // 0x8020 -> 010800000000007C00
        let input = hex::decode("010800000000007C00").unwrap();
        let result = FirmwareDownloadInfoDid::try_from(input.as_slice()).unwrap();
        assert_eq!(&result.to_bytes()[..], &input[..]);

        assert_eq!(result.supported, true);
        assert_eq!(result.address, 0x08000000);
        assert_eq!(result.max_size, 0x00007C00);
    }

    #[test]
    fn test_0x8021() {
        // 0x8021 -> 000000000200000000
        let input = hex::decode("000000000200000000").unwrap();
        let result = LogUploadInfoDid::try_from(input.as_slice()).unwrap();
        assert_eq!(&result.to_bytes()[..], &input[..]);

        assert_eq!(result.supported, false);
        assert_eq!(result.address, 0x00000002);
        assert_eq!(result.size, 0x00000000);
    }

    #[test]
    fn test_0x8200() {
        // 0x8200 -> A005D007
        let input = hex::decode("A005D007").unwrap();
        let result = SerialDid::try_from(input.as_slice()).unwrap();
        assert_eq!(result.to_bytes(), result.serial);
    }

    #[test]
    fn test_0x8201() {
        // 0x8201 -> 50FF68064884534917540887
        let input = hex::decode("50FF68064884534917540887").unwrap();
        let result = DeviceIdDid::try_from(input.as_slice()).unwrap();
        assert_eq!(&result.to_bytes()[..], &input[..]);
    }

    #[test]
    fn test_0x8202() {
        // 0x8202 -> A0094770000047703235313100003030
        let input = hex::decode("A0094770000047703235313100003030").unwrap();
        let result = SoloEncryptedConfigAndIdDid::try_from(input.as_slice()).unwrap();
        assert_eq!(result.to_bytes(), result.unknown);
    }

    #[test]
    fn test_0x8203() {
        // 0x8203 -> 000000B1000000B1000000A3010101
        let input = hex::decode("000000B1000000B1000000A3010101").unwrap();
        let result = SoloO2CellCalibrationDid::try_from(input.as_slice()).unwrap();
        assert_eq!(&result.to_bytes()[..], &input[..]);
        assert_eq!(result.o2_calibrations[0], 0x000000B1);
        assert_eq!(result.o2_calibrations[1], 0x000000B1);
        assert_eq!(result.o2_calibrations[2], 0x000000A3);
        assert_eq!(result.calibration_valid[0], true);
        assert_eq!(result.calibration_valid[1], true);
        assert_eq!(result.calibration_valid[2], true);
    }

    #[test]
    fn test_0x8205() {
        // 0x8205 -> 000F000200F0000288004803
        let input = hex::decode("000F000200F0000288004803").unwrap();
        let result = SoloO2CellFactoryCalibrationDid::try_from(input.as_slice()).unwrap();
        assert_eq!(&result.to_bytes()[..], &input[..]);

        assert_eq!(result.cells[0], 0x000F0002); // 983042
        assert_eq!(result.cells[1], 0x00F00002); // 15728642
        assert_eq!(result.cells[2], 0x88004803); // 2281735171
    }

    #[test]
    fn test_0x8209() {
        // 0x8209 -> B8211756
        let input = hex::decode("B8211756").unwrap();
        let result = FirmwareCrcDid::try_from(input.as_slice()).unwrap();
        assert_eq!(result.crc, 0xB8211756);
        assert_eq!(&result.to_bytes()[..], &input[..]);
    }

    #[test]
    fn test_0x820a() {
        // 0x820a -> 30303639
        let input = hex::decode("30303639").unwrap();
        let result = SoloAdcVrefCalibrationDid::try_from(input.as_slice()).unwrap();
        assert_eq!(&result.to_bytes()[..], &input[..]);
    }

    #[test]
    fn test_0x820b() {
        // 0x820b -> 8AFC3656
        let input = hex::decode("8AFC3656").unwrap();
        let result = SoloConfigDid::try_from(input.as_slice()).unwrap();
        assert_eq!(&result.to_bytes()[..], &input[..]);
        assert_eq!(result.battery_voltage_min, 148);
    }
    #[test]
    fn test_internal_calibration_data() {
        // Real data from device: 000000E4000000E4000000CC010101
        let data = hex::decode("000000E4000000E4000000CC010101").unwrap();
        let cal_data = SoloO2CellCalibrationDid::try_from(data.as_slice()).unwrap();
        assert_eq!(cal_data.o2_calibrations[0], 228);
        assert_eq!(cal_data.o2_calibrations[1], 228);
        assert_eq!(cal_data.o2_calibrations[2], 204);
        assert_eq!(cal_data.calibration_valid[0], true);
        assert_eq!(cal_data.calibration_valid[1], true);
        assert_eq!(cal_data.calibration_valid[2], true);

        // Test round-trip
        let bytes = cal_data.to_bytes();
        assert_eq!(&bytes[..], &data[..]);
    }
}
