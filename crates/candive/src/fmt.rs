use core::fmt;

use crate::units::{
    CentiMillivolt, Decibar, Decivolt, Fo2, Milliamp, Millibar, Millisecond, Millivolt, PpO2Deci,
};

impl fmt::Display for Millibar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} mbar", self.raw())
    }
}

impl fmt::Display for Decibar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} dbar", self.raw())
    }
}

impl fmt::Display for Millivolt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} mV", self.raw())
    }
}

impl fmt::Display for Millisecond {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ms", self.raw())
    }
}

impl fmt::Display for Milliamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} mA", self.raw())
    }
}

impl fmt::Display for Decivolt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let v = self.raw();
        write!(f, "{}.{} V", v / 10, v % 10)
    }
}

impl fmt::Display for PpO2Deci {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let v = self.raw();
        write!(f, "{}.{} ppO₂", v / 10, v % 10)
    }
}

impl fmt::Display for Fo2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let v = self.raw();
        write!(f, "0.{:02} FO₂", v) // if 99 means 0.99
    }
}

impl fmt::Display for CentiMillivolt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let v = self.raw();
        write!(f, "{}.{:02} mV", v / 100, v % 100)
    }
}
