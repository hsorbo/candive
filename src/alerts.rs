#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandsetAlert {
    //Sent by handset
    ShutdownWhileBluetooth,
    ShutdownWhileDiving,
    ShutdownWhileFwUpgrade,
    ShutdownWhileUnknown,
}

impl HandsetAlert {
    pub fn from_u16(v: u16) -> Option<Self> {
        match v {
            0x21 => Some(Self::ShutdownWhileBluetooth),
            0x23 => Some(Self::ShutdownWhileDiving),
            0x27 => Some(Self::ShutdownWhileFwUpgrade),
            0x28 => Some(Self::ShutdownWhileUnknown),
            _ => None,
        }
    }

    pub fn to_u16(self) -> u16 {
        match self {
            Self::ShutdownWhileBluetooth => 0x21,
            Self::ShutdownWhileDiving => 0x23,
            Self::ShutdownWhileFwUpgrade => 0x27,
            Self::ShutdownWhileUnknown => 0x28,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TempAlert {
    //Sent by probably temp probe
    TempProbeFailed,
}

impl TempAlert {
    pub fn from_u16(v: u16) -> Option<Self> {
        match v {
            0x201 => Some(Self::TempProbeFailed),
            _ => None,
        }
    }

    pub fn to_u16(self) -> u16 {
        match self {
            Self::TempProbeFailed => 0x201,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoloAlert {
    SoloCellStatusMaskZero,
    SoloSetpointTimeout,
    SoloSetpointUpdateTimeout,
    SoloPPO2Below004PPO2,
    SoloSetpointOutOfRange,

    SoloFwCrcFailed,
    SoloFwCrcReset,
    SoloReadSettingsFailed,
    SoloSpiFlashBusy,

    IsotpSingleFrameSendFailed, // 0x1502
    IsotpFlowControlTimeout,    // 0x1503
    IsotpBusySingleFrame,       // 0x1504
    IsotpBusyFirstFrame,        // 0x1505

    UdsTransferDownloadOutOfRange,     // 0x1581
    UdsTransferDownloadProgFailed,     // 0x1582
    UdsTransferIncorrectMessageLength, // 0x1583
    UdsTransferDownloadWrongSequence,  // 0x1584
    UdsTransferWrongBlockSequence,     // 0x1586
    UdsTransferRequestSequenceError,   // 0x1587
    UdsTransferExitFailed,             // 0x1588
    UdsTransferNoBlocksTransferred,    // 0x1589
    UdsTransferCrcVerifyFailed,        // 0x158A
    UdsTransferCrcMismatch,            // 0x158B
    UdsTransferVerifyProgFailed,       // 0x158C
    UdsTransferUploadFailed,           // 0x158D
    UdsTransferTimeout,                // 0x158E
}

impl SoloAlert {
    pub fn from_u16(v: u16) -> Option<Self> {
        match v {
            0x101 => Some(Self::SoloCellStatusMaskZero),
            0x103 => Some(Self::SoloSetpointTimeout),
            0x104 => Some(Self::SoloSetpointUpdateTimeout),
            0x108 => Some(Self::SoloPPO2Below004PPO2),
            0x109 => Some(Self::SoloSetpointOutOfRange),
            0x400 => Some(Self::SoloFwCrcFailed),
            0x401 => Some(Self::SoloFwCrcReset),
            0x402 => Some(Self::SoloReadSettingsFailed),
            0x403 => Some(Self::SoloSpiFlashBusy),
            0x1502 => Some(Self::IsotpSingleFrameSendFailed),
            0x1503 => Some(Self::IsotpFlowControlTimeout),
            0x1504 => Some(Self::IsotpBusySingleFrame),
            0x1505 => Some(Self::IsotpBusyFirstFrame),
            0x1581 => Some(Self::UdsTransferDownloadOutOfRange),
            0x1582 => Some(Self::UdsTransferDownloadProgFailed),
            0x1583 => Some(Self::UdsTransferIncorrectMessageLength),
            0x1584 => Some(Self::UdsTransferDownloadWrongSequence),
            0x1586 => Some(Self::UdsTransferWrongBlockSequence),
            0x1587 => Some(Self::UdsTransferRequestSequenceError),
            0x1588 => Some(Self::UdsTransferExitFailed),
            0x1589 => Some(Self::UdsTransferNoBlocksTransferred),
            0x158A => Some(Self::UdsTransferCrcVerifyFailed),
            0x158B => Some(Self::UdsTransferCrcMismatch),
            0x158C => Some(Self::UdsTransferVerifyProgFailed),
            0x158D => Some(Self::UdsTransferUploadFailed),
            0x158E => Some(Self::UdsTransferTimeout),
            _ => None,
        }
    }

    pub fn to_u16(self) -> u16 {
        match self {
            Self::SoloCellStatusMaskZero => 0x101,
            Self::SoloSetpointTimeout => 0x103,
            Self::SoloSetpointUpdateTimeout => 0x104,
            Self::SoloPPO2Below004PPO2 => 0x108,
            Self::SoloSetpointOutOfRange => 0x109,
            Self::SoloFwCrcFailed => 0x400,
            Self::SoloFwCrcReset => 0x401,
            Self::SoloReadSettingsFailed => 0x402,
            Self::SoloSpiFlashBusy => 0x403,
            Self::IsotpSingleFrameSendFailed => 0x1502,
            Self::IsotpFlowControlTimeout => 0x1503,
            Self::IsotpBusySingleFrame => 0x1504,
            Self::IsotpBusyFirstFrame => 0x1505,
            Self::UdsTransferDownloadOutOfRange => 0x1581,
            Self::UdsTransferDownloadProgFailed => 0x1582,
            Self::UdsTransferIncorrectMessageLength => 0x1583,
            Self::UdsTransferDownloadWrongSequence => 0x1584,
            Self::UdsTransferWrongBlockSequence => 0x1586,
            Self::UdsTransferRequestSequenceError => 0x1587,
            Self::UdsTransferExitFailed => 0x1588,
            Self::UdsTransferNoBlocksTransferred => 0x1589,
            Self::UdsTransferCrcVerifyFailed => 0x158A,
            Self::UdsTransferCrcMismatch => 0x158B,
            Self::UdsTransferVerifyProgFailed => 0x158C,
            Self::UdsTransferUploadFailed => 0x158D,
            Self::UdsTransferTimeout => 0x158E,
        }
    }
}

//TODO: There is a error case 0x302 handled by handset
