pub mod daikin;

#[cfg(target_os = "espidf")]
pub mod rmt;

pub use daikin::*;

pub trait IrTransmitter: Send + Sync {
    fn send_command(&mut self, cmd: &DaikinCommand) -> Result<(), String>;
}

/// Simulated mock IR transmitter for host testing.
#[derive(Debug, Clone, Default)]
pub struct MockTransmitter {
    pub last_command: Option<DaikinCommand>,
    pub send_count: usize,
}

impl MockTransmitter {
    pub fn new() -> Self {
        Self::default()
    }
}

impl IrTransmitter for MockTransmitter {
    fn send_command(&mut self, cmd: &DaikinCommand) -> Result<(), String> {
        self.last_command = Some(*cmd);
        self.send_count += 1;
        log::info!(
            "[IR Mock] Sent Daikin command #{}: power={}, mode={:?}, temp={}°C",
            self.send_count,
            cmd.power,
            cmd.mode,
            cmd.temp_celsius
        );
        Ok(())
    }
}
