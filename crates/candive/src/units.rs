#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Millibar(u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Decibar(u16);

/// mV × 100 (e.g. 5524 => 55.24 mV)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CentiMillivolt(u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Millivolt(u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Decivolt(u8);

/// ppO2 × 10 (e.g. 70 => 0.7)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PpO2Deci(u8);

// x100 (99 => 0.99)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Fo2(u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Millisecond(u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Milliamp(u16);

impl Millibar {
    pub const fn new(v: u16) -> Self {
        Self(v)
    }
    pub const fn raw(self) -> u16 {
        self.0
    }
}
impl Decibar {
    pub const fn new(v: u16) -> Self {
        Self(v)
    }
    pub const fn raw(self) -> u16 {
        self.0
    }
}
impl Millivolt {
    pub const fn new(v: u8) -> Self {
        Self(v)
    }
    pub const fn raw(self) -> u8 {
        self.0
    }
}
impl Millisecond {
    pub const fn new(v: u16) -> Self {
        Self(v)
    }
    pub const fn raw(self) -> u16 {
        self.0
    }
}
impl Milliamp {
    pub const fn new(v: u16) -> Self {
        Self(v)
    }
    pub const fn raw(self) -> u16 {
        self.0
    }
}
impl Decivolt {
    pub const fn new(v: u8) -> Self {
        Self(v)
    }
    pub const fn raw(self) -> u8 {
        self.0
    }
}
impl PpO2Deci {
    pub const fn new(v: u8) -> Self {
        Self(v)
    }
    pub const fn raw(self) -> u8 {
        self.0
    }
}
impl Fo2 {
    pub const fn new(v: u8) -> Self {
        Self(v)
    }
    pub const fn raw(self) -> u8 {
        self.0
    }
}
impl CentiMillivolt {
    pub const fn new(v: u16) -> Self {
        Self(v)
    }
    pub const fn raw(self) -> u16 {
        self.0
    }
}

impl From<u16> for Millibar {
    fn from(v: u16) -> Self {
        Self::new(v)
    }
}
impl From<Millibar> for u16 {
    fn from(v: Millibar) -> u16 {
        v.raw()
    }
}

impl From<u16> for Decibar {
    fn from(v: u16) -> Self {
        Self::new(v)
    }
}
impl From<Decibar> for u16 {
    fn from(v: Decibar) -> u16 {
        v.raw()
    }
}

impl From<u8> for Millivolt {
    fn from(v: u8) -> Self {
        Self::new(v)
    }
}
impl From<Millivolt> for u8 {
    fn from(v: Millivolt) -> u8 {
        v.raw()
    }
}

impl From<u16> for Millisecond {
    fn from(v: u16) -> Self {
        Self::new(v)
    }
}
impl From<Millisecond> for u16 {
    fn from(v: Millisecond) -> u16 {
        v.raw()
    }
}

impl From<u16> for Milliamp {
    fn from(v: u16) -> Self {
        Self::new(v)
    }
}
impl From<Milliamp> for u16 {
    fn from(v: Milliamp) -> u16 {
        v.raw()
    }
}

impl From<u8> for Decivolt {
    fn from(v: u8) -> Self {
        Self::new(v)
    }
}
impl From<Decivolt> for u8 {
    fn from(v: Decivolt) -> u8 {
        v.raw()
    }
}

impl From<u8> for PpO2Deci {
    fn from(v: u8) -> Self {
        Self::new(v)
    }
}
impl From<PpO2Deci> for u8 {
    fn from(v: PpO2Deci) -> u8 {
        v.raw()
    }
}

impl From<u8> for Fo2 {
    fn from(v: u8) -> Self {
        Self::new(v)
    }
}
impl From<Fo2> for u8 {
    fn from(v: Fo2) -> u8 {
        v.raw()
    }
}

impl From<u16> for CentiMillivolt {
    fn from(v: u16) -> Self {
        Self::new(v)
    }
}
impl From<CentiMillivolt> for u16 {
    fn from(v: CentiMillivolt) -> u16 {
        v.raw()
    }
}
