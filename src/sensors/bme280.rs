//! BME280 environmental sensor driver (I2C) with Bosch Sensortec compensation.

use super::TemperatureHumiditySensor;

#[cfg(target_os = "espidf")]
use esp_idf_hal::i2c::I2cDriver;

pub const BME280_I2C_ADDR_PRIMARY: u8 = 0x76;
pub const BME280_I2C_ADDR_SECONDARY: u8 = 0x77;
pub const BME280_CHIP_ID: u8 = 0x60;

#[derive(Debug, Clone, Copy, Default)]
pub struct Bme280Calibration {
    pub dig_t1: u16,
    pub dig_t2: i16,
    pub dig_t3: i16,
    pub dig_h1: u8,
    pub dig_h2: i16,
    pub dig_h3: u8,
    pub dig_h4: i16,
    pub dig_h5: i16,
    pub dig_h6: i8,
}

pub struct EspBme280<'d> {
    #[cfg(target_os = "espidf")]
    driver: Option<I2cDriver<'d>>,
    #[cfg(target_os = "espidf")]
    addr: u8,
    #[cfg(target_os = "espidf")]
    calib: Bme280Calibration,
    #[cfg(target_os = "espidf")]
    initialized: bool,
    #[cfg(not(target_os = "espidf"))]
    _phantom: std::marker::PhantomData<&'d ()>,
}

impl<'d> EspBme280<'d> {
    #[cfg(target_os = "espidf")]
    pub fn new(driver: I2cDriver<'d>) -> Self {
        let mut s = Self {
            driver: Some(driver),
            addr: BME280_I2C_ADDR_PRIMARY,
            calib: Bme280Calibration::default(),
            initialized: false,
        };
        let _ = s.init_sensor();
        s
    }

    #[cfg(not(target_os = "espidf"))]
    pub fn new() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }

    #[cfg(target_os = "espidf")]
    fn init_sensor(&mut self) -> Result<(), String> {
        let driver = self.driver.as_mut().ok_or("No I2C driver")?;

        // Probe 0x76 then 0x77
        let mut found_addr = None;
        for &addr in &[BME280_I2C_ADDR_PRIMARY, BME280_I2C_ADDR_SECONDARY] {
            let mut id = [0u8; 1];
            if driver.write_read(addr, &[0xD0], &mut id, 100).is_ok() && id[0] == BME280_CHIP_ID {
                found_addr = Some(addr);
                break;
            }
        }

        let addr = found_addr.ok_or("BME280 sensor not detected on I2C bus")?;
        self.addr = addr;

        // Read Temperature Calibration (0x88..0x8D)
        let mut t_buf = [0u8; 6];
        driver
            .write_read(addr, &[0x88], &mut t_buf, 100)
            .map_err(|e| format!("I2C read calib T error: {:?}", e))?;
        self.calib.dig_t1 = u16::from_le_bytes([t_buf[0], t_buf[1]]);
        self.calib.dig_t2 = i16::from_le_bytes([t_buf[2], t_buf[3]]);
        self.calib.dig_t3 = i16::from_le_bytes([t_buf[4], t_buf[5]]);

        // Read Humidity Calibration (0xA1 and 0xE1..0xE7)
        let mut h1 = [0u8; 1];
        driver
            .write_read(addr, &[0xA1], &mut h1, 100)
            .map_err(|e| format!("I2C read calib H1 error: {:?}", e))?;
        self.calib.dig_h1 = h1[0];

        let mut h_buf = [0u8; 7];
        driver
            .write_read(addr, &[0xE1], &mut h_buf, 100)
            .map_err(|e| format!("I2C read calib H error: {:?}", e))?;
        self.calib.dig_h2 = i16::from_le_bytes([h_buf[0], h_buf[1]]);
        self.calib.dig_h3 = h_buf[2];
        self.calib.dig_h4 = ((h_buf[3] as i16) << 4) | ((h_buf[4] as i16) & 0x0F);
        self.calib.dig_h5 = ((h_buf[5] as i16) << 4) | (((h_buf[4] as i16) >> 4) & 0x0F);
        self.calib.dig_h6 = h_buf[6] as i8;

        // Configure sensor: oversampling x1, normal mode
        driver
            .write(addr, &[0xF2, 0x01], 100)
            .map_err(|e| format!("I2C config ctrl_hum error: {:?}", e))?;
        driver
            .write(addr, &[0xF4, 0x27], 100)
            .map_err(|e| format!("I2C config ctrl_meas error: {:?}", e))?;
        driver
            .write(addr, &[0xF5, 0xA0], 100)
            .map_err(|e| format!("I2C config config error: {:?}", e))?;

        self.initialized = true;
        log::info!(
            "[BME280] Sensor initialized successfully at I2C address 0x{:02X}",
            addr
        );
        Ok(())
    }
}

#[cfg(not(target_os = "espidf"))]
impl<'d> Default for EspBme280<'d> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'d> TemperatureHumiditySensor for EspBme280<'d> {
    fn read(&mut self) -> Result<(f32, f32), String> {
        #[cfg(target_os = "espidf")]
        {
            if !self.initialized {
                self.init_sensor()?;
            }

            let driver = self.driver.as_mut().ok_or("No I2C driver")?;
            let addr = self.addr;

            // Read temperature (0xFA..0xFC) and humidity (0xFD..0xFE)
            let mut raw_data = [0u8; 5];
            driver
                .write_read(addr, &[0xFA], &mut raw_data, 100)
                .map_err(|e| format!("I2C read telemetry error: {:?}", e))?;

            let raw_temp = (((raw_data[0] as i32) << 12)
                | ((raw_data[1] as i32) << 4)
                | (((raw_data[2] as i32) >> 4) & 0x0F)) as i32;

            let raw_hum = (((raw_data[3] as i32) << 8) | (raw_data[4] as i32)) as i32;

            let (temp, t_fine) = compensate_temperature(raw_temp, &self.calib);
            let hum = compensate_humidity(raw_hum, t_fine, &self.calib);

            if temp < -20.0 || temp > 85.0 || hum < 0.0 || hum > 100.0 {
                return Err("BME280 reading out of physical range".to_string());
            }

            Ok((temp, hum))
        }

        #[cfg(not(target_os = "espidf"))]
        {
            Err("BME280 not available on host target".to_string())
        }
    }
}

/// Bosch Sensortec temperature compensation formula
pub fn compensate_temperature(raw_t: i32, calib: &Bme280Calibration) -> (f32, i32) {
    let var1 = (((raw_t >> 3) - ((calib.dig_t1 as i32) << 1)) * (calib.dig_t2 as i32)) >> 11;
    let var2 =
        (((((raw_t >> 4) - (calib.dig_t1 as i32)) * ((raw_t >> 4) - (calib.dig_t1 as i32))) >> 12)
            * (calib.dig_t3 as i32))
            >> 14;
    let t_fine = var1 + var2;
    let temp = ((t_fine * 5 + 128) >> 8) as f32 / 100.0;
    (temp, t_fine)
}

/// Bosch Sensortec humidity compensation formula
pub fn compensate_humidity(raw_h: i32, t_fine: i32, calib: &Bme280Calibration) -> f32 {
    let mut v_x1_u32r = t_fine - 76800;
    v_x1_u32r =
        ((((raw_h << 14) - ((calib.dig_h4 as i32) << 20) - ((calib.dig_h5 as i32) * v_x1_u32r))
            + 16384)
            >> 15)
            * (((((((v_x1_u32r * (calib.dig_h6 as i32)) >> 10)
                * (((v_x1_u32r * (calib.dig_h3 as i32)) >> 11) + 32768))
                >> 10)
                + 2097152)
                * (calib.dig_h2 as i32)
                + 8192)
                >> 14);
    v_x1_u32r =
        v_x1_u32r - (((((v_x1_u32r >> 15) * (v_x1_u32r >> 15)) >> 7) * (calib.dig_h1 as i32)) >> 4);
    let v_x1 = if v_x1_u32r < 0 { 0 } else { v_x1_u32r };
    let v_x2 = if v_x1 > 419430400 { 419430400 } else { v_x1 };
    (v_x2 >> 12) as f32 / 1024.0
}
