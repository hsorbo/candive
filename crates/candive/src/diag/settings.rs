#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserSettingDidError {
    TooShort { needed: usize },
    TooLong { max: usize },
    InvalidFormat,
    UnknownDid(u16),
    BadSettingType(u8),
    BadEnumIndex(u8),
}

impl UserSettingDidError {
    pub fn length_mismatch(actual: usize, expected: usize) -> Self {
        if actual < expected {
            UserSettingDidError::TooShort { needed: expected }
        } else {
            UserSettingDidError::TooLong { max: expected }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum UserSettingType {
    /// When divisor=0: displays as hexadecimal
    /// When divisor!=0: displays as (value / divisor / 100) in decimal
    Integer = 0,
    Selection = 1,
    /// Same behavior as Integer
    Scaled = 2,
}

impl TryFrom<u8> for UserSettingType {
    type Error = UserSettingDidError;

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        Ok(match v {
            0 => UserSettingType::Integer,
            1 => UserSettingType::Selection,
            2 => UserSettingType::Scaled,
            other => return Err(UserSettingDidError::BadSettingType(other)),
        })
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum UserSettingDid {
    Count,
    Info { index: u8 },
    ReadState { index: u8 },
    Enum { index: u8, enum_index: u8 },
    WriteInput { index: u8 },
}

impl UserSettingDid {
    const USER_SETTING_COUNT: u16 = 0x9100;
    const USER_SETTING_INFO: u16 = 0x9110;
    const USER_SETTING_VALUE: u16 = 0x9130;
    const USER_SETTING_LABEL: u16 = 0x9150;
    const USER_SETTING_SAVE: u16 = 0x9350;

    const ENUM_INDEX_OFFSET: u8 = 5;
    const MAX_ENUM_INDEX: u8 = 10;

    pub fn to_did(self) -> u16 {
        match self {
            UserSettingDid::Count => Self::USER_SETTING_COUNT,
            UserSettingDid::Info { index } => Self::USER_SETTING_INFO + (index as u16),
            UserSettingDid::ReadState { index } => Self::USER_SETTING_VALUE + (index as u16),
            UserSettingDid::Enum { enum_index, index } => {
                Self::USER_SETTING_LABEL + (enum_index as u16) + ((index as u16) << 4)
            }
            UserSettingDid::WriteInput { index } => Self::USER_SETTING_SAVE + (index as u16),
        }
    }
}

impl TryFrom<u16> for UserSettingDid {
    type Error = UserSettingDidError;

    fn try_from(did: u16) -> Result<Self, Self::Error> {
        let kind = did & 0xFFF0;
        let index = (did & 0x000F) as u8;
        let option_nibble = ((did & 0x00F0) >> 4) as u8;

        if did == Self::USER_SETTING_COUNT {
            return Ok(UserSettingDid::Count);
        }

        if kind == Self::USER_SETTING_INFO {
            return Ok(UserSettingDid::Info { index });
        }

        if kind == Self::USER_SETTING_VALUE {
            return Ok(UserSettingDid::ReadState { index });
        }

        let label_kind_min = Self::USER_SETTING_LABEL;
        let label_kind_max = 0x91F0;

        if kind >= label_kind_min && kind <= label_kind_max {
            let index_from_nibble = option_nibble
                .checked_sub(Self::ENUM_INDEX_OFFSET)
                .ok_or(UserSettingDidError::BadEnumIndex(option_nibble))?;

            if index_from_nibble > Self::MAX_ENUM_INDEX {
                return Err(UserSettingDidError::BadEnumIndex(option_nibble));
            }

            return Ok(UserSettingDid::Enum {
                enum_index: index,
                index: index_from_nibble,
            });
        }

        if kind == Self::USER_SETTING_SAVE {
            return Ok(UserSettingDid::WriteInput { index });
        }

        Err(UserSettingDidError::UnknownDid(did))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SettingValue {
    SelectionIndex {
        max_index: u8,
        current_index: u8,
    },

    IntegerHex {
        value: u32,
        min: u32,
        max: u32,
    },

    IntegerScaled {
        value: u32,
        divisor: u32,
        min: u32,
        max: u32,
    },
}

impl SettingValue {
    pub fn encode(&self) -> [u8; 16] {
        let mut buf = [0u8; 16];
        match self {
            SettingValue::SelectionIndex {
                max_index,
                current_index,
            } => {
                buf[7] = *max_index;
                buf[15] = *current_index;
            }
            SettingValue::IntegerHex { value, min, max } => {
                buf[0..4].copy_from_slice(&min.to_be_bytes());
                buf[4..8].copy_from_slice(&max.to_be_bytes());
                buf[8..12].copy_from_slice(&[0, 0, 0, 0]);
                buf[12..16].copy_from_slice(&value.to_be_bytes());
            }
            SettingValue::IntegerScaled {
                value,
                divisor,
                min,
                max,
            } => {
                buf[0..4].copy_from_slice(&min.to_be_bytes());
                buf[4..8].copy_from_slice(&max.to_be_bytes());
                buf[8..12].copy_from_slice(&divisor.to_be_bytes());
                buf[12..16].copy_from_slice(&value.to_be_bytes());
            }
        }
        buf
    }

    pub fn decode(type_hint: UserSettingType, data: &[u8; 16]) -> Self {
        let min = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let max = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let divisor = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
        let value = u32::from_be_bytes([data[12], data[13], data[14], data[15]]);

        match type_hint {
            UserSettingType::Selection => {
                let max_index = data[7];
                let current_index = data[15];
                SettingValue::SelectionIndex {
                    max_index,
                    current_index,
                }
            }
            UserSettingType::Integer | UserSettingType::Scaled => {
                if divisor == 0 {
                    SettingValue::IntegerHex { value, min, max }
                } else {
                    SettingValue::IntegerScaled {
                        value,
                        divisor,
                        min,
                        max,
                    }
                }
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct UserSettingInput {
    pub len: u8,
    pub bytes: [u8; 8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserSettingPayload {
    Count(u8),
    Info {
        name: [u8; 10],
        editable: bool,
        kind: UserSettingType,
    },
    State([u8; 16]),
    Input(UserSettingInput),
    Enum([u8; 8]),
}

impl UserSettingPayload {
    pub fn decode(ident: UserSettingDid, data: &[u8]) -> Result<Self, UserSettingDidError> {
        match ident {
            UserSettingDid::Count => {
                if data.len() < 1 {
                    return Err(UserSettingDidError::TooShort { needed: 1 });
                }
                Ok(UserSettingPayload::Count(data[0]))
            }

            UserSettingDid::Info { .. } => {
                if data.len() < 12 {
                    return Err(UserSettingDidError::TooShort { needed: 12 });
                }

                let name: [u8; 10] = data[0..10]
                    .try_into()
                    .map_err(|_| UserSettingDidError::length_mismatch(data.len(), 10))?;
                let kind_byte = data[10];
                let editable_byte = data[11];

                let kind = UserSettingType::try_from(kind_byte)?;

                Ok(UserSettingPayload::Info {
                    name,
                    editable: editable_byte != 0,
                    kind,
                })
            }

            UserSettingDid::ReadState { .. } => {
                let data_array: [u8; 16] = data
                    .try_into()
                    .map_err(|_| UserSettingDidError::length_mismatch(data.len(), 16))?;
                Ok(UserSettingPayload::State(data_array))
            }

            UserSettingDid::Enum { .. } => {
                let name: [u8; 8] = data
                    .try_into()
                    .map_err(|_| UserSettingDidError::length_mismatch(data.len(), 8))?;
                Ok(UserSettingPayload::Enum(name))
            }

            UserSettingDid::WriteInput { .. } => {
                if data.len() < 4 {
                    return Err(UserSettingDidError::TooShort { needed: 4 });
                }
                if data.len() > 8 {
                    return Err(UserSettingDidError::TooLong { max: 8 });
                }
                let len = data.len() as u8;
                let mut bytes = [0u8; 8];
                bytes[0..len as usize].copy_from_slice(data);
                Ok(UserSettingPayload::Input(UserSettingInput { len, bytes }))
            }
        }
    }

    pub fn encode(&self, buf: &mut [u8]) -> Result<usize, UserSettingDidError> {
        match self {
            UserSettingPayload::Count(count) => {
                if buf.len() < 1 {
                    return Err(UserSettingDidError::TooShort { needed: 1 });
                }
                buf[0] = *count;
                Ok(1)
            }
            UserSettingPayload::Info {
                name,
                editable,
                kind,
            } => {
                let name_len = name.len();
                let total_needed = name_len + 2;
                if buf.len() < total_needed {
                    return Err(UserSettingDidError::TooShort {
                        needed: total_needed,
                    });
                }
                buf[..name_len].copy_from_slice(name);
                buf[name_len] = *kind as u8;
                buf[name_len + 1] = if *editable { 1 } else { 0 };
                Ok(name_len + 2)
            }
            UserSettingPayload::State(raw_data) => {
                if buf.len() < 16 {
                    return Err(UserSettingDidError::TooShort { needed: 16 });
                }
                buf[0..16].copy_from_slice(raw_data);
                Ok(16)
            }
            UserSettingPayload::Input(input) => {
                let len = input.len as usize;
                if buf.len() < len {
                    return Err(UserSettingDidError::TooShort { needed: len });
                }
                buf[0..len].copy_from_slice(&input.bytes[0..len]);
                Ok(len)
            }
            UserSettingPayload::Enum(name) => {
                let len = name.len();
                if buf.len() < len {
                    return Err(UserSettingDidError::TooShort { needed: len });
                }
                buf[..len].copy_from_slice(name);
                Ok(len)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex::FromHex;

    #[test]
    fn info_roundtrip_to_uds() {
        let payload = UserSettingPayload::Info {
            name: *b"SOLO_SPEED",
            editable: true,
            kind: UserSettingType::Selection,
        };

        let mut data: [u8; 100] = [0; 100];
        let len = payload.encode(&mut data).unwrap();

        //00629110534F4C4F5F53504545440101
        let bytes: Vec<u8> = Vec::from_hex("534F4C4F5F53504545440101").unwrap();
        assert_eq!(&data[..len], &bytes);
    }

    #[test]
    fn name_roundtrip_to_uds() {
        let payload = UserSettingPayload::Enum(*b"PID_5SEC");
        let mut data: [u8; 100] = [0; 100];
        let len = payload.encode(&mut data).unwrap();
        //006291515049445f35534543
        let bytes: Vec<u8> = Vec::from_hex("5049445f35534543").unwrap();
        assert_eq!(&data[..len], &bytes);
    }

    #[test]
    fn enum_did_roundtrip() {
        for index in 0u8..=10 {
            for enum_index in 0u8..=15 {
                let original = UserSettingDid::Enum { index, enum_index };
                let did = original.to_did();

                if let Ok(decoded) = UserSettingDid::try_from(did) {
                    assert_eq!(decoded, original, "did=0x{did:04X}");
                }
            }
        }
    }

    #[test]
    fn save_payload_rountrip() {
        let save = UserSettingDid::WriteInput { index: 0 };
        let expected_bytes = b"ABCDEFGH";

        let payload = UserSettingPayload::Input(UserSettingInput {
            len: 8,
            bytes: *expected_bytes,
        });

        let mut buf = [0u8; 16];
        let len = payload.encode(&mut buf).unwrap();

        // Only decode the actual encoded data, not the entire buffer
        match UserSettingPayload::decode(save, &buf[..len]).unwrap() {
            UserSettingPayload::Input(value) => {
                assert_eq!(value.len, 8);
                assert_eq!(value.bytes, *expected_bytes);
            }
            other => panic!("unexpected decoded variant: {other:?}"),
        }
    }

    #[test]
    fn all_variants_roundtrip() {
        let variants = vec![
            UserSettingDid::Count,
            UserSettingDid::Info { index: 0 },
            UserSettingDid::Info { index: 15 },
            UserSettingDid::ReadState { index: 0 },
            UserSettingDid::ReadState { index: 15 },
            UserSettingDid::Enum {
                index: 0,
                enum_index: 0,
            },
            UserSettingDid::Enum {
                index: 10,
                enum_index: 15,
            },
            UserSettingDid::WriteInput { index: 0 },
            UserSettingDid::WriteInput { index: 15 },
        ];

        for variant in variants {
            let did = variant.to_did();
            let decoded = UserSettingDid::try_from(did)
                .unwrap_or_else(|e| panic!("Failed to decode {variant:?} (0x{did:04X}): {e:?}"));
            assert_eq!(
                decoded, variant,
                "Roundtrip failed for {variant:?} (0x{did:04X})"
            );
        }
    }

    #[test]
    fn write_input_too_long_error() {
        // Test that data longer than 8 bytes returns error
        let write_input = UserSettingDid::WriteInput { index: 0 };
        let too_long_data = b"ABCDEFGHI"; // 9 bytes

        let result = UserSettingPayload::decode(write_input, too_long_data);
        assert!(matches!(
            result,
            Err(UserSettingDidError::TooLong { max: 8 })
        ));
    }
}
