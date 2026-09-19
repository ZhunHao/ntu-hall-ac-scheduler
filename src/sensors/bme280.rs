//! BME280 environmental sensor driver using modern ESP-IDF v5 I2C Master driver (`driver/i2c_master.h`).
//!
//! Includes Bosch Sensortec integer fixed-point calibration and compensation algorithms.

use super::TemperatureHumiditySensor;

#[cfg(target_os = "espidf")]
use esp_idf_sys::*;

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

pub struct EspBme280 {
    #[cfg(target_os = "espidf")]
    bus_handle: i2c_master_bus_handle_t,
    #[cfg(target_os = "espidf")]
    dev_handle: i2c_master_dev_handle_t,
    #[cfg(target_os = "espidf")]
    addr: u8,
    #[cfg(target_os = "espidf")]
    calib: Bme280Calibration,
    #[cfg(target_os = "espidf")]
    initialized: bool,
}

unsafe impl Send for EspBme280 {}
unsafe impl Sync for EspBme280 {}

impl EspBme280 {
    #[cfg(target_os = "espidf")]
    pub fn new(sda_gpio: i32, scl_gpio: i32) -> Result<Self, String> {
        let mut flags = i2c_master_bus_config_t__bindgen_ty_1::default();
        flags.set_enable_internal_pullup(1);

        let bus_config = i2c_master_bus_config_t {
            i2c_port: 0,
            sda_io_num: sda_gpio as gpio_num_t,
            scl_io_num: scl_gpio as gpio_num_t,
            clk_source: soc_periph_i2c_clk_src_t_I2C_CLK_SRC_DEFAULT,
            glitch_ignore_cnt: 7,
            intr_priority: 0,
            trans_queue_depth: 0,
            flags,
        };

        let mut bus_handle: i2c_master_bus_handle_t = core::ptr::null_mut();
        unsafe {
            esp!(i2c_new_master_bus(&bus_config, &mut bus_handle))
                .map_err(|e| format!("Failed to create modern I2C master bus: {:?}", e))?;
        }

        let mut s = Self {
            bus_handle,
            dev_handle: core::ptr::null_mut(),
            addr: BME280_I2C_ADDR_PRIMARY,
            calib: Bme280Calibration::default(),
            initialized: false,
        };

        s.init_sensor()?;
        Ok(s)
    }

    #[cfg(not(target_os = "espidf"))]
    pub fn new() -> Self {
        Self {}
    }

    #[cfg(target_os = "espidf")]
    fn init_sensor(&mut self) -> Result<(), String> {
        // Probe 0x76 then 0x77
        let mut found_addr = None;
        let mut found_dev: i2c_master_dev_handle_t = core::ptr::null_mut();

        for &addr in &[BME280_I2C_ADDR_PRIMARY, BME280_I2C_ADDR_SECONDARY] {
            let dev_config = i2c_device_config_t {
                dev_addr_length: i2c_addr_bit_len_t_I2C_ADDR_BIT_LEN_7,
                device_address: addr as u16,
                scl_speed_hz: 100_000,
                scl_wait_us: 0,
                flags: Default::default(),
            };

            let mut dev_handle: i2c_master_dev_handle_t = core::ptr::null_mut();
            let add_res =
                unsafe { i2c_master_bus_add_device(self.bus_handle, &dev_config, &mut dev_handle) };
            if add_res != ESP_OK {
                continue;
            }

            let reg = [0xD0u8];
            let mut id = [0u8; 1];
            let read_res = unsafe {
                i2c_master_transmit_receive(
                    dev_handle,
                    reg.as_ptr(),
                    1,
                    id.as_mut_ptr(),
                    1,
                    100, // 100 ms timeout
                )
            };

            if read_res == ESP_OK && id[0] == BME280_CHIP_ID {
                found_addr = Some(addr);
                found_dev = dev_handle;
                break;
            } else {
                unsafe {
                    i2c_master_bus_rm_device(dev_handle);
                }
            }
        }

        let addr = found_addr.ok_or("BME280 sensor not detected on modern I2C bus")?;
        self.addr = addr;
        self.dev_handle = found_dev;

        // Read Temperature Calibration (0x88..0x8D)
        let reg_t = [0x88u8];
        let mut t_buf = [0u8; 6];
        unsafe {
            esp!(i2c_master_transmit_receive(
                self.dev_handle,
                reg_t.as_ptr(),
                1,
                t_buf.as_mut_ptr(),
                6,
                100,
            ))
            .map_err(|e| format!("I2C read calib T error: {:?}", e))?;
        }
        self.calib.dig_t1 = u16::from_le_bytes([t_buf[0], t_buf[1]]);
        self.calib.dig_t2 = i16::from_le_bytes([t_buf[2], t_buf[3]]);
        self.calib.dig_t3 = i16::from_le_bytes([t_buf[4], t_buf[5]]);

        // Read Humidity Calibration (0xA1 and 0xE1..0xE7)
        let reg_h1 = [0xA1u8];
        let mut h1 = [0u8; 1];
        unsafe {
            esp!(i2c_master_transmit_receive(
                self.dev_handle,
                reg_h1.as_ptr(),
                1,
                h1.as_mut_ptr(),
                1,
                100,
            ))
            .map_err(|e| format!("I2C read calib H1 error: {:?}", e))?;
        }
        self.calib.dig_h1 = h1[0];

        let reg_h = [0xE1u8];
        let mut h_buf = [0u8; 7];
        unsafe {
            esp!(i2c_master_transmit_receive(
                self.dev_handle,
                reg_h.as_ptr(),
                1,
                h_buf.as_mut_ptr(),
                7,
                100,
            ))
            .map_err(|e| format!("I2C read calib H error: {:?}", e))?;
        }
        self.calib.dig_h2 = i16::from_le_bytes([h_buf[0], h_buf[1]]);
        self.calib.dig_h3 = h_buf[2];
        self.calib.dig_h4 = ((h_buf[3] as i16) << 4) | ((h_buf[4] as i16) & 0x0F);
        self.calib.dig_h5 = ((h_buf[5] as i16) << 4) | (((h_buf[4] as i16) >> 4) & 0x0F);
        self.calib.dig_h6 = h_buf[6] as i8;

        // Configure sensor: oversampling x1, normal mode
        let cfg1 = [0xF2u8, 0x01];
        let cfg2 = [0xF4u8, 0x27];
        let cfg3 = [0xF5u8, 0xA0];
        unsafe {
            esp!(i2c_master_transmit(self.dev_handle, cfg1.as_ptr(), 2, 100))
                .map_err(|e| format!("I2C config ctrl_hum error: {:?}", e))?;
            esp!(i2c_master_transmit(self.dev_handle, cfg2.as_ptr(), 2, 100))
                .map_err(|e| format!("I2C config ctrl_meas error: {:?}", e))?;
            esp!(i2c_master_transmit(self.dev_handle, cfg3.as_ptr(), 2, 100))
                .map_err(|e| format!("I2C config config error: {:?}", e))?;
        }

        self.initialized = true;
        log::info!(
            "[BME280] Modern ESP-IDF v5 I2C initialized at address 0x{:02X}",
            addr
        );
        Ok(())
    }
}

#[cfg(target_os = "espidf")]
impl Drop for EspBme280 {
    fn drop(&mut self) {
        unsafe {
            if !self.dev_handle.is_null() {
                let _ = i2c_master_bus_rm_device(self.dev_handle);
            }
            if !self.bus_handle.is_null() {
                let _ = i2c_del_master_bus(self.bus_handle);
            }
        }
    }
}

#[cfg(not(target_os = "espidf"))]
impl Default for EspBme280 {
    fn default() -> Self {
        Self::new()
    }
}

impl TemperatureHumiditySensor for EspBme280 {
    fn read(&mut self) -> Result<(f32, f32), String> {
        #[cfg(target_os = "espidf")]
        {
            if !self.initialized {
                self.init_sensor()?;
            }

            // Read temperature (0xFA..0xFC) and humidity (0xFD..0xFE)
            let reg = [0xFAu8];
            let mut raw_data = [0u8; 5];
            unsafe {
                esp!(i2c_master_transmit_receive(
                    self.dev_handle,
                    reg.as_ptr(),
                    1,
                    raw_data.as_mut_ptr(),
                    5,
                    100,
                ))
                .map_err(|e| format!("I2C read telemetry error: {:?}", e))?;
            }

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

/// Bosch Sensortec 32-bit integer temperature compensation algorithm.
pub fn compensate_temperature(raw_temp: i32, calib: &Bme280Calibration) -> (f32, i32) {
    let var1 = (((raw_temp >> 3) - ((calib.dig_t1 as i32) << 1)) * (calib.dig_t2 as i32)) >> 11;
    let var2 = (((((raw_temp >> 4) - (calib.dig_t1 as i32))
        * ((raw_temp >> 4) - (calib.dig_t1 as i32)))
        >> 12)
        * (calib.dig_t3 as i32))
        >> 14;
    let t_fine = var1 + var2;
    let temp = ((t_fine * 5 + 128) >> 8) as f32 / 100.0;
    (temp, t_fine)
}

/// Bosch Sensortec 32-bit integer humidity compensation algorithm.
pub fn compensate_humidity(raw_hum: i32, t_fine: i32, calib: &Bme280Calibration) -> f32 {
    let mut v_x1_u32r = t_fine - 76800;
    v_x1_u32r =
        ((((raw_hum << 14) - ((calib.dig_h4 as i32) << 20) - ((calib.dig_h5 as i32) * v_x1_u32r))
            + 16384)
            >> 15)
            * (((((((v_x1_u32r * (calib.dig_h6 as i32)) >> 10)
                * (((v_x1_u32r * (calib.dig_h3 as i32)) >> 11) + 32768))
                >> 10)
                + 2097152)
                * (calib.dig_h2 as i32)
                + 8192)
                >> 14);
    v_x1_u32r -= ((((v_x1_u32r >> 15) * (v_x1_u32r >> 15)) >> 7) * (calib.dig_h1 as i32)) >> 4;
    let v_x1_u32r = if v_x1_u32r < 0 { 0 } else { v_x1_u32r };
    let v_x1_u32r = if v_x1_u32r > 419430400 {
        419430400
    } else {
        v_x1_u32r
    };
    (v_x1_u32r >> 12) as f32 / 1024.0
}
