pub mod did;
pub mod settings;
pub mod solo;

pub struct Stm32Crc32 {
    crc: u32,
}

impl Stm32Crc32 {
    const POLY: u32 = 0x04C1_1DB7;

    pub fn new() -> Self {
        Self { crc: 0xFFFF_FFFF }
    }

    pub fn reset(&mut self) {
        self.crc = 0xFFFF_FFFF;
    }

    pub fn append(&mut self, data: &[u8]) {
        for chunk in data.chunks(4) {
            let mut word = 0u32;
            for (i, &byte) in chunk.iter().enumerate() {
                word |= (byte as u32) << (i * 8);
            }

            self.crc ^= word;

            for _ in 0..32 {
                self.crc = if (self.crc & 0x8000_0000) != 0 {
                    (self.crc << 1) ^ Self::POLY
                } else {
                    self.crc << 1
                };
            }
        }
    }

    pub fn checksum(&self) -> u32 {
        self.crc
    }

    pub fn stm32_crc32(data: &[u8]) -> u32 {
        let mut c = Self::new();
        c.append(data);
        c.checksum()
    }
}

impl Default for Stm32Crc32 {
    fn default() -> Self {
        Self::new()
    }
}
