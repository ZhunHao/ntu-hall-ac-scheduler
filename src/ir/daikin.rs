//! Daikin 280-bit (35 bytes) AC Infrared Protocol implementation.
//!
//! Encodes AC states (Power, Mode, Temperature, Fan, Swing) into the exact
//! 35-byte Daikin packet with section checksums and pulse/space timings (38 kHz).

pub const DAIKIN_STATE_LENGTH: usize = 35;
pub const DAIKIN_SECTION1_LEN: usize = 8;
pub const DAIKIN_SECTION2_LEN: usize = 8;
pub const DAIKIN_SECTION3_LEN: usize = 19;

// Carrier & Timing constants (in microseconds)
pub const CARRIER_FREQ_HZ: u32 = 38_000;
pub const DUTY_PERCENT: u32 = 33;

pub const HDR_MARK_US: u32 = 3650;
pub const HDR_SPACE_US: u32 = 1623;
pub const BIT_MARK_US: u32 = 428;
pub const ZERO_SPACE_US: u32 = 428;
pub const ONE_SPACE_US: u32 = 1280;
pub const GAP_SPACE_US: u32 = 29_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationMode {
    Auto = 0b000,
    Dry = 0b010,
    Cool = 0b011,
    Heat = 0b100,
    Fan = 0b110,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FanSpeed {
    Min = 1,
    Med = 3,
    Max = 5,
    Auto = 0b1010,  // 10
    Quiet = 0b1011, // 11
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DaikinCommand {
    pub power: bool,
    pub mode: OperationMode,
    pub temp_celsius: u8,
    pub fan: FanSpeed,
    pub swing_v: bool,
}

impl Default for DaikinCommand {
    fn default() -> Self {
        Self {
            power: false,
            mode: OperationMode::Cool,
            temp_celsius: 24,
            fan: FanSpeed::Auto,
            swing_v: false,
        }
    }
}

impl DaikinCommand {
    pub fn cool_at(temp: u8) -> Self {
        Self {
            power: true,
            mode: OperationMode::Cool,
            temp_celsius: temp.clamp(10, 32),
            fan: FanSpeed::Auto,
            swing_v: false,
        }
    }

    pub fn off() -> Self {
        Self {
            power: false,
            mode: OperationMode::Cool,
            temp_celsius: 24,
            fan: FanSpeed::Auto,
            swing_v: false,
        }
    }
}

/// Represents the raw 35-byte Daikin packet state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaikinPacket {
    raw: [u8; DAIKIN_STATE_LENGTH],
}

impl Default for DaikinPacket {
    fn default() -> Self {
        let mut p = Self {
            raw: [0u8; DAIKIN_STATE_LENGTH],
        };
        p.reset();
        p
    }
}

impl DaikinPacket {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_command(cmd: &DaikinCommand) -> Self {
        let mut p = Self::new();
        p.set_power(cmd.power);
        p.set_mode(cmd.mode);
        p.set_temp(cmd.temp_celsius);
        p.set_fan(cmd.fan);
        p.set_swing_v(cmd.swing_v);
        p.recompute_checksums();
        p
    }

    pub fn reset(&mut self) {
        self.raw.fill(0);

        // Section 1: bytes 0..7
        self.raw[0] = 0x11;
        self.raw[1] = 0xDA;
        self.raw[2] = 0x27;
        self.raw[4] = 0xC5;

        // Section 2: bytes 8..15
        self.raw[8] = 0x11;
        self.raw[9] = 0xDA;
        self.raw[10] = 0x27;
        self.raw[12] = 0x42;

        // Section 3: bytes 16..34
        self.raw[16] = 0x11;
        self.raw[17] = 0xDA;
        self.raw[18] = 0x27;
        self.raw[21] = 0x49; // Default power + mode
        self.raw[22] = 0x1E; // Default temp (15 C * 2 = 30)
        self.raw[24] = 0xB0; // Default fan
        self.raw[27] = 0x06;
        self.raw[28] = 0x60;
        self.raw[31] = 0xC0;

        self.recompute_checksums();
    }

    pub fn set_power(&mut self, on: bool) {
        if on {
            self.raw[21] |= 0x01;
        } else {
            self.raw[21] &= !0x01;
        }
    }

    pub fn get_power(&self) -> bool {
        (self.raw[21] & 0x01) != 0
    }

    pub fn set_mode(&mut self, mode: OperationMode) {
        // Mode is stored in bits 4..6 of byte 21. Bit 3 is always 1.
        self.raw[21] = (self.raw[21] & !0x78) | 0x08 | ((mode as u8 & 0x07) << 4);
    }

    pub fn set_temp(&mut self, celsius: u8) {
        let clamped = celsius.clamp(10, 32);
        self.raw[22] = clamped * 2;
    }

    pub fn get_temp(&self) -> u8 {
        self.raw[22] / 2
    }

    pub fn set_fan(&mut self, fan: FanSpeed) {
        let fan_val = match fan {
            FanSpeed::Quiet => 0x0B,
            FanSpeed::Auto => 0x0A,
            FanSpeed::Min => 2 + 1,
            FanSpeed::Med => 2 + 3,
            FanSpeed::Max => 2 + 5,
        };
        self.raw[24] = (self.raw[24] & 0x0F) | ((fan_val & 0x0F) << 4);
    }

    pub fn set_swing_v(&mut self, on: bool) {
        let val = if on { 0x0F } else { 0x00 };
        self.raw[24] = (self.raw[24] & 0xF0) | (val & 0x0F);
    }

    pub fn raw_bytes(&self) -> &[u8; DAIKIN_STATE_LENGTH] {
        &self.raw
    }

    pub fn recompute_checksums(&mut self) {
        // Checksum 1: sum of bytes 0..6
        let sum1: u32 = self.raw[0..7].iter().map(|&b| b as u32).sum();
        self.raw[7] = (sum1 & 0xFF) as u8;

        // Checksum 2: sum of bytes 8..14
        let sum2: u32 = self.raw[8..15].iter().map(|&b| b as u32).sum();
        self.raw[15] = (sum2 & 0xFF) as u8;

        // Checksum 3: sum of bytes 16..33
        let sum3: u32 = self.raw[16..34].iter().map(|&b| b as u32).sum();
        self.raw[34] = (sum3 & 0xFF) as u8;
    }

    pub fn verify_checksums(&self) -> bool {
        let sum1: u32 = self.raw[0..7].iter().map(|&b| b as u32).sum();
        if self.raw[7] != (sum1 & 0xFF) as u8 {
            return false;
        }

        let sum2: u32 = self.raw[8..15].iter().map(|&b| b as u32).sum();
        if self.raw[15] != (sum2 & 0xFF) as u8 {
            return false;
        }

        let sum3: u32 = self.raw[16..34].iter().map(|&b| b as u32).sum();
        self.raw[34] == (sum3 & 0xFF) as u8
    }

    /// Converts the packet to raw microsecond pulse/space durations (mark, space, mark, space...).
    ///
    /// Framing follows IRremoteESP8266's `IRsend::sendDaikin`: every segment ends with a footer
    /// mark followed by the gap. The footer mark terminates the last data bit's space; without
    /// it the receiver cannot decode that bit (the checksum MSB of each section).
    pub fn to_pulses(&self) -> Vec<(u32, u32)> {
        let mut pulses = Vec::with_capacity(300);

        // Preamble: 5 zero bits + footer
        for _ in 0..5 {
            pulses.push((BIT_MARK_US, ZERO_SPACE_US));
        }
        pulses.push(Self::FOOTER);

        // Section 1 (bytes 0..8)
        Self::encode_section(&self.raw[0..8], &mut pulses);

        // Section 2 (bytes 8..16)
        Self::encode_section(&self.raw[8..16], &mut pulses);

        // Section 3 (bytes 16..35)
        Self::encode_section(&self.raw[16..35], &mut pulses);

        pulses
    }

    fn encode_section(bytes: &[u8], pulses: &mut Vec<(u32, u32)>) {
        // Section Header
        pulses.push((HDR_MARK_US, HDR_SPACE_US));

        // Data bytes transmitted LSB first
        for &byte in bytes {
            for bit in 0..8 {
                let space = if (byte & (1 << bit)) != 0 {
                    ONE_SPACE_US
                } else {
                    ZERO_SPACE_US
                };
                pulses.push((BIT_MARK_US, space));
            }
        }

        pulses.push(Self::FOOTER);
    }

    /// Segment footer: `mark(kDaikinBitMark)` then `space(kDaikinZeroSpace + kDaikinGap)`.
    const FOOTER: (u32, u32) = (BIT_MARK_US, ZERO_SPACE_US + GAP_SPACE_US);
}
