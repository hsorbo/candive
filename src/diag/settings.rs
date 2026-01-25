#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserSettingDidError {
    TooShort { needed: usize },
    BadLength { expected: usize },
    InvalidFormat,
    UnknownDid(u16),
    BadSettingType(u8),
    BadEnumIndex(u8),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum UserSettingType {
    Text = 0,
    Selection = 1,
    Number = 2,
}

impl TryFrom<u8> for UserSettingType {
    type Error = UserSettingDidError;

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        Ok(match v {
            0 => UserSettingType::Text,
            1 => UserSettingType::Selection,
            2 => UserSettingType::Number,
            other => return Err(UserSettingDidError::BadSettingType(other)),
        })
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum UserSettingDid {
    Count,
    Info { index: u8 },
    Value { index: u8 },
    Enum { index: u8, enum_index: u8 },
    Save { index: u8 },
}

impl UserSettingDid {
    const USER_SETTING_COUNT: u16 = 0x9100;
    const USER_SETTING_INFO: u16 = 0x9110;
    const USER_SETTING_VALUE: u16 = 0x9130;
    const USER_SETTING_LABEL: u16 = 0x9150;
    const USER_SETTING_SAVE: u16 = 0x9350;

    pub fn to_did(self) -> u16 {
        match self {
            UserSettingDid::Count => Self::USER_SETTING_COUNT,
            UserSettingDid::Info { index } => Self::USER_SETTING_INFO + (index as u16),
            UserSettingDid::Value { index } => Self::USER_SETTING_VALUE + (index as u16),
            UserSettingDid::Enum { enum_index, index } => {
                Self::USER_SETTING_LABEL + (enum_index as u16) + ((index as u16) << 4)
            }
            UserSettingDid::Save { index } => Self::USER_SETTING_SAVE + (index as u16),
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
            return Ok(UserSettingDid::Value { index });
        }

        let label_kind_min = Self::USER_SETTING_LABEL;
        let label_kind_max = 0x9200;

        if kind >= label_kind_min && kind <= label_kind_max {
            let enum_index = option_nibble
                .checked_sub(5)
                .ok_or(UserSettingDidError::BadEnumIndex(option_nibble))?;

            if enum_index > 10 {
                return Err(UserSettingDidError::BadEnumIndex(option_nibble));
            }

            return Ok(UserSettingDid::Enum { enum_index, index });
        }

        if kind == Self::USER_SETTING_SAVE {
            return Ok(UserSettingDid::Save { index });
        }

        Err(UserSettingDidError::UnknownDid(did))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct SettingValue {
    pub max: u64,
    pub value: [u8; 8],
}

impl SettingValue {
    pub fn as_u64(&self) -> u64 {
        u64::from_be_bytes(self.value)
    }

    pub fn as_bytes(&self) -> &[u8; 8] {
        &self.value
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserSettingPayload {
    Count(u8),
    Info {
        name: [u8; 10],
        editable: bool,
        kind: UserSettingType,
    },
    GetValue(SettingValue),
    SetValue {
        value: [u8; 8],
    },
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

                let name_end = data.len() - 2;
                //todo: verify no variable length
                let name: [u8; 10] = data[0..10]
                    .try_into()
                    .map_err(|_| UserSettingDidError::BadLength { expected: 10 })?;
                let kind_byte = data[name_end];
                let editable_byte = data[name_end + 1];

                let kind = UserSettingType::try_from(kind_byte)?;

                Ok(UserSettingPayload::Info {
                    name,
                    editable: editable_byte != 0,
                    kind,
                })
            }

            UserSettingDid::Value { .. } => {
                if data.len() < 16 {
                    return Err(UserSettingDidError::TooShort { needed: 16 });
                }

                let max = u64::from_be_bytes(data[0..8].try_into().expect("slice len checked"));
                let value: [u8; 8] = data[8..16].try_into().expect("slice len checked");

                Ok(UserSettingPayload::GetValue(SettingValue { max, value }))
            }

            UserSettingDid::Enum { .. } => {
                if data.len() != 8 {
                    return Err(UserSettingDidError::BadLength { expected: 8 });
                }
                let name: [u8; 8] = data
                    .try_into()
                    .map_err(|_| UserSettingDidError::BadLength { expected: 8 })?;
                Ok(UserSettingPayload::Enum(name))
            }

            UserSettingDid::Save { .. } => {
                if data.len() < 16 {
                    return Err(UserSettingDidError::TooShort { needed: 16 });
                }
                let value: [u8; 8] = data[8..16].try_into().expect("slice len checked");
                Ok(UserSettingPayload::SetValue { value })
            }
        }
    }

    pub fn encode(&self, buf: &mut [u8]) -> usize {
        //todo: assert size
        match self {
            UserSettingPayload::Count(count) => {
                buf[0] = *count;
                1
            }
            UserSettingPayload::Info {
                name,
                editable,
                kind,
            } => {
                let name_len = name.len();
                buf[..name_len].copy_from_slice(name);
                buf[name_len] = *kind as u8;
                buf[name_len + 1] = if *editable { 1 } else { 0 };
                name_len + 2
            }
            UserSettingPayload::GetValue(setting_value) => {
                let max_bytes = setting_value.max.to_be_bytes();
                buf[0..8].copy_from_slice(&max_bytes);
                buf[8..16].copy_from_slice(&setting_value.value);
                16
            }
            UserSettingPayload::SetValue { value } => {
                buf[0..8].copy_from_slice(value);
                8
            }
            UserSettingPayload::Enum(name) => {
                let len = name.len();
                buf[..len].copy_from_slice(name);
                len
            }
        }
    }
}

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
        let len = payload.encode(&mut data);

        //00629110534F4C4F5F53504545440101
        let bytes: Vec<u8> = Vec::from_hex("534F4C4F5F53504545440101").unwrap();
        assert_eq!(&data[..len], &bytes);
    }

    #[test]
    fn name_roundtrip_to_uds() {
        let payload = UserSettingPayload::Enum(*b"PID_5SEC");
        let mut data: [u8; 100] = [0; 100];
        let len = payload.encode(&mut data);
        //006291515049445f35534543
        let bytes: Vec<u8> = Vec::from_hex("5049445f35534543").unwrap();
        assert_eq!(&data[..len], &bytes);
    }
}
