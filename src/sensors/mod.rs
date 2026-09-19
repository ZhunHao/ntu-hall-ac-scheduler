pub mod bme280;

pub trait TemperatureHumiditySensor: Send + Sync {
    fn read(&mut self) -> Result<(f32, f32), String>;
}

/// Simulated sensor for host testing and simulator.
pub struct MockSensor {
    pub temp: f32,
    pub humidity: f32,
}

impl MockSensor {
    pub fn new(temp: f32, humidity: f32) -> Self {
        Self { temp, humidity }
    }
}

impl Default for MockSensor {
    fn default() -> Self {
        Self::new(27.4, 68.0)
    }
}

impl TemperatureHumiditySensor for MockSensor {
    fn read(&mut self) -> Result<(f32, f32), String> {
        Ok((self.temp, self.humidity))
    }
}
