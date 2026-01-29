use core::ops::RangeInclusive;

use crate::diag::did::DidDecodeError;

#[derive(Debug, Clone)]
pub struct UploadRegion {
    pub addr_range: RangeInclusive<u32>,
    /// Required address alignment in bytes (0 = no requirement)
    pub addr_align: u32,
    /// Minimum allowed size in bytes (inclusive)
    pub size_min: u32,
    /// Maximum allowed size in bytes (inclusive)
    pub size_max: u32,
    /// Required size alignment in bytes (0 = no requirement)
    pub size_align: u32,
}

impl UploadRegion {
    pub const MMC_START: Self = Self {
        addr_range: 0xC200_0080..=0xC200_0FFF,
        addr_align: 8,
        size_min: 8,
        size_max: 0xFFFF_FFFF,
        size_align: 8,
    };

    pub const MMC_LOG: Self = Self {
        addr_range: 0xC300_1000..=0xC3FF_FFFF,
        addr_align: 0,
        size_min: 12,
        size_max: 0x00FF_F000,
        size_align: 12,
    };

    pub const MCU_DEVINFO: Self = Self {
        addr_range: 0xC500_0000..=0xC500_007F,
        addr_align: 0,
        size_min: 1,
        size_max: 0x80,
        size_align: 0,
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UdsSecuritySeed {
    /// CRC32 of transferred data (4 bytes at offset 0-3, little-endian)
    pub crc32_result: u32,
    /// Length field (1 byte at offset 4, always 0x10 = 16)
    pub length: u8,
    /// RTC timestamp used for log encryption (4 bytes at offset 5-8, little-endian)
    pub rtc_timestamp: u32,
    /// Device ID from MCU unique ID (12 bytes at offset 9-20)
    pub device_id: [u8; 12],
}

impl UdsSecuritySeed {
    pub fn to_bytes(&self) -> [u8; 21] {
        let mut result = [0u8; 21];
        result[0..4].copy_from_slice(&self.crc32_result.to_le_bytes());
        result[4] = self.length;
        result[5..9].copy_from_slice(&self.rtc_timestamp.to_le_bytes());
        result[9..21].copy_from_slice(&self.device_id);
        result
    }
}

impl TryFrom<&[u8]> for UdsSecuritySeed {
    type Error = DidDecodeError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.len() < 21 {
            return Err(DidDecodeError::TooShort { needed: 21 });
        }

        let crc32_result = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let length = bytes[4];
        let rtc_timestamp = u32::from_le_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]);
        let mut device_id = [0u8; 12];
        device_id.copy_from_slice(&bytes[9..21]);

        Ok(UdsSecuritySeed {
            crc32_result,
            length,
            rtc_timestamp,
            device_id,
        })
    }
}

pub const LOG_ENTRY_SIZE: u32 = 12;

#[derive(Debug, Clone, Copy)]
pub struct LogEntry {
    pub kind: u8,
    pub payload: [u8; 8],
}

pub struct LogEntryIterator<'a> {
    data: &'a [u8],
    offset: usize,
    current_kind: u8,
}

impl<'a> LogEntryIterator<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            offset: 0,
            current_kind: 0x00,
        }
    }
}

impl<'a> Iterator for LogEntryIterator<'a> {
    type Item = LogEntry;

    fn next(&mut self) -> Option<Self::Item> {
        while self.offset + 12 <= self.data.len() {
            let entry = &self.data[self.offset..self.offset + 12];
            self.offset += 12;

            if entry.iter().all(|&b| b == 0xFF) || entry.iter().all(|&b| b == 0x00) {
                self.current_kind = entry[10];
                continue;
            }

            let mut payload = [0u8; 8];
            payload.copy_from_slice(&entry[..8]);
            let kind = self.current_kind;
            self.current_kind = entry[10];

            return Some(LogEntry { kind, payload });
        }
        None
    }
}

pub trait DesEncryptor {
    fn encrypt_block(&self, block: &mut [u8; 8]);
}

pub struct LogDecryptor {
    key_material: [[u8; 8]; 3],
    seed: u64,
    pos: u8,
}

impl LogDecryptor {
    const LCG_MULT: u64 = 0x10A860C1;
    const LCG_MOD: u64 = 0xFFFFFFFB;

    pub fn new<E: DesEncryptor>(des: &E, device_id: &[u8], timestamp: u32) -> Self {
        let mut key_material = [[0u8; 8]; 3];

        key_material[0][0..8].copy_from_slice(&device_id[0..8]);
        key_material[1][0..4].copy_from_slice(&device_id[8..12]);
        key_material[1][4..8].copy_from_slice(&timestamp.to_le_bytes());
        key_material[2].copy_from_slice(&[0xda, 0x65, 0x20, 0x33, 0xc8, 0x57, 0x40, 0xd3]);

        for i in 0..3 {
            des.encrypt_block(&mut key_material[i]);
        }

        Self {
            key_material,
            seed: timestamp as u64,
            pos: 0,
        }
    }

    #[inline]
    pub fn decrypt(&mut self, buf: &mut [u8]) {
        for b in buf {
            let block_index = (self.pos >> 3) as usize;
            let byte_index = (self.pos & 7) as usize;
            let key_byte = self.key_material[block_index][byte_index];

            self.seed = (self.seed.wrapping_mul(Self::LCG_MULT)) % Self::LCG_MOD;
            let keystream_byte = (self.seed & 0xFF) as u8;

            *b ^= key_byte ^ keystream_byte;

            self.pos += 1;
            if self.pos == 24 {
                self.pos = 0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uds_security_seed_real_data() {
        //from flash?
        let buffer = [
            0x87, 0xf7, 0xca, 0x4f, 0x10, 0xc9, 0xc1, 0x02, 0x00, 0x50, 0xff, 0x68, 0x06, 0x48,
            0x84, 0x53, 0x49, 0x17, 0x54, 0x08, 0x87,
        ];

        let parsed = UdsSecuritySeed::try_from(buffer.as_slice()).unwrap();
        assert_eq!(parsed.crc32_result, 0x4fcaf787);
        assert_eq!(parsed.length, 0x10);
        assert_eq!(parsed.rtc_timestamp, 0x0002c1c9);
        assert_eq!(
            hex::encode(&parsed.device_id).to_uppercase(),
            "50FF68064884534917540887"
        );
    }
}
