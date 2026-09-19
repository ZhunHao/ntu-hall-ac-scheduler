//! Modern ESP-IDF v5 RMT (Remote Control) TX driver for 38 kHz modulated Daikin IR transmission.
//!
//! Uses ESP-IDF v5's `driver/rmt_tx.h` API directly, bypassing legacy v4 drivers.

use super::daikin::{DaikinCommand, DaikinPacket, CARRIER_FREQ_HZ, DUTY_PERCENT};
use super::IrTransmitter;
use esp_idf_sys::*;
use std::thread::sleep;
use std::time::Duration;

const BURST_COUNT: u32 = 5;
const BURST_SPACING: Duration = Duration::from_millis(200);

pub struct EspRmtTransmitter {
    channel: rmt_channel_handle_t,
    encoder: rmt_encoder_handle_t,
}

// Safety: The RMT channel and encoder handles are thread-safe to send between threads.
unsafe impl Send for EspRmtTransmitter {}
unsafe impl Sync for EspRmtTransmitter {}

impl EspRmtTransmitter {
    pub fn new(gpio_num: i32) -> Result<Self, EspError> {
        let tx_config = rmt_tx_channel_config_t {
            gpio_num: gpio_num as gpio_num_t,
            clk_src: soc_periph_rmt_clk_src_t_RMT_CLK_SRC_DEFAULT,
            resolution_hz: 1_000_000, // 1 MHz -> 1 tick = 1 µs
            mem_block_symbols: 64,
            trans_queue_depth: 4,
            intr_priority: 0,
            flags: Default::default(),
        };

        let mut channel: rmt_channel_handle_t = core::ptr::null_mut();
        esp!(unsafe { rmt_new_tx_channel(&tx_config, &mut channel) })?;

        let carrier_config = rmt_carrier_config_t {
            frequency_hz: CARRIER_FREQ_HZ,
            duty_cycle: (DUTY_PERCENT as f32) / 100.0,
            flags: Default::default(),
        };
        esp!(unsafe { rmt_apply_carrier(channel, &carrier_config) })?;
        esp!(unsafe { rmt_enable(channel) })?;

        let copy_config = rmt_copy_encoder_config_t {};
        let mut encoder: rmt_encoder_handle_t = core::ptr::null_mut();
        esp!(unsafe { rmt_new_copy_encoder(&copy_config, &mut encoder) })?;

        log::info!(
            "[IR RMT] Modern ESP-IDF v5 RMT TX initialized on GPIO {} (38 kHz, 1 MHz clock)",
            gpio_num
        );

        Ok(Self { channel, encoder })
    }
}

impl Drop for EspRmtTransmitter {
    fn drop(&mut self) {
        unsafe {
            let _ = rmt_disable(self.channel);
            let _ = rmt_del_channel(self.channel);
            let _ = rmt_del_encoder(self.encoder);
        }
    }
}

impl IrTransmitter for EspRmtTransmitter {
    fn send_command(&mut self, cmd: &DaikinCommand) -> Result<(), String> {
        let packet = DaikinPacket::from_command(cmd);
        let pulses = packet.to_pulses();

        // Convert (mark_us, space_us) pairs into hardware 32-bit RMT symbols
        // bit 0..14 = duration0 (mark), bit 15 = level0 (1), bit 16..30 = duration1 (space), bit 31 = level1 (0)
        let symbols: Vec<rmt_symbol_word_t> = pulses
            .iter()
            .map(|&(mark, space)| {
                let m = (mark & 0x7FFF) as u32;
                let s = (space & 0x7FFF) as u32;
                rmt_symbol_word_t {
                    val: m | (1 << 15) | (s << 16),
                }
            })
            .collect();

        let transmit_config = rmt_transmit_config_t {
            loop_count: 0,
            flags: Default::default(),
        };

        for i in 0..BURST_COUNT {
            log::info!("[IR RMT] Sending burst {}/{}...", i + 1, BURST_COUNT);
            unsafe {
                esp!(rmt_transmit(
                    self.channel,
                    self.encoder,
                    symbols.as_ptr() as *const _,
                    symbols.len() * core::mem::size_of::<rmt_symbol_word_t>(),
                    &transmit_config,
                ))
                .map_err(|e| format!("RMT transmit error: {:?}", e))?;

                // Wait until this burst has finished transmitting
                esp!(rmt_tx_wait_all_done(self.channel, -1))
                    .map_err(|e| format!("RMT wait error: {:?}", e))?;
            }

            if i + 1 < BURST_COUNT {
                sleep(BURST_SPACING);
            }
        }
        log::info!("[IR RMT] Burst finished successfully.");

        Ok(())
    }
}
