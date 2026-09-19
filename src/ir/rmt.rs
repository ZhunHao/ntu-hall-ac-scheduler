//! ESP-IDF RMT (Remote Control) peripheral driver for 38 kHz modulated Daikin IR transmission.
//!
//! Uses esp-idf-hal's legacy RMT driver (`rmt-legacy` feature in Cargo.toml).

use super::daikin::{DaikinCommand, DaikinPacket, CARRIER_FREQ_HZ, DUTY_PERCENT};
use super::IrTransmitter;
use esp_idf_hal::gpio::OutputPin;
use esp_idf_hal::peripheral::Peripheral;
use esp_idf_hal::rmt::config::{CarrierConfig, DutyPercent, TransmitConfig};
use esp_idf_hal::rmt::{PinState, Pulse, PulseTicks, RmtChannel, Symbol, TxRmtDriver};
use esp_idf_hal::sys::EspError;
use esp_idf_hal::units::Hertz;
use std::thread::sleep;
use std::time::Duration;

/// 80 MHz RMT source clock (PLL_F80M on ESP32-C6) / 80 = 1 MHz, so 1 tick = 1 µs.
const CLOCK_DIVIDER: u8 = 80;
const BURST_COUNT: u32 = 5;
const BURST_SPACING: Duration = Duration::from_millis(200);

pub struct EspRmtTransmitter<'d> {
    driver: TxRmtDriver<'d>,
}

impl<'d> EspRmtTransmitter<'d> {
    pub fn new<C: RmtChannel>(
        channel: impl Peripheral<P = C> + 'd,
        pin: impl Peripheral<P = impl OutputPin> + 'd,
    ) -> Result<Self, EspError> {
        let carrier = CarrierConfig::new()
            .frequency(Hertz(CARRIER_FREQ_HZ))
            .duty_percent(DutyPercent::new(DUTY_PERCENT as u8)?);
        let config = TransmitConfig::new()
            .clock_divider(CLOCK_DIVIDER)
            .carrier(Some(carrier));

        let driver = TxRmtDriver::new(channel, pin, &config)?;
        Ok(Self { driver })
    }
}

/// Converts a duration in µs to RMT ticks, rejecting values above the 15-bit (32 767) limit.
fn ticks(us: u32) -> Result<PulseTicks, String> {
    u16::try_from(us)
        .ok()
        .and_then(|t| PulseTicks::new(t).ok())
        .ok_or_else(|| format!("IR pulse of {us} µs exceeds the RMT tick limit"))
}

/// Converts (mark_us, space_us) pairs to RMT symbols (carrier on for mark, off for space).
fn to_symbols(pulses: &[(u32, u32)]) -> Result<Vec<Symbol>, String> {
    pulses
        .iter()
        .map(|&(mark, space)| {
            Ok(Symbol::new(
                Pulse::new(PinState::High, ticks(mark)?),
                Pulse::new(PinState::Low, ticks(space)?),
            ))
        })
        .collect()
}

impl<'d> IrTransmitter for EspRmtTransmitter<'d> {
    fn send_command(&mut self, cmd: &DaikinCommand) -> Result<(), String> {
        let packet = DaikinPacket::from_command(cmd);
        let symbols = to_symbols(&packet.to_pulses())?;

        // Blast 5 times; each send blocks until the full frame (~450 ms) is out, then waits 200 ms
        for i in 0..BURST_COUNT {
            log::info!("[IR RMT] Sending burst {}/{}...", i + 1, BURST_COUNT);
            self.driver
                .start_iter_blocking(symbols.iter().copied())
                .map_err(|e| format!("RMT transmit error: {:?}", e))?;
            if i + 1 < BURST_COUNT {
                sleep(BURST_SPACING);
            }
        }
        log::info!("[IR RMT] Burst finished successfully.");

        Ok(())
    }
}
