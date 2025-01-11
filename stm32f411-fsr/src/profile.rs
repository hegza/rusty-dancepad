use heapless::Vec;

pub(crate) struct Profile {
    /// ADC0
    pub(crate) pa0: Option<SensorConfig>,
    /// ADC1
    pub(crate) pa1: Option<SensorConfig>,
    /// ADC2
    pub(crate) pa2: Option<SensorConfig>,
    /// ADC3
    pub(crate) pa3: Option<SensorConfig>,
    /// ADC4
    pub(crate) pa4: Option<SensorConfig>,
    /// ADC5
    pub(crate) pa5: Option<SensorConfig>,
    /// ADC6
    pub(crate) pa6: Option<SensorConfig>,
    /// ADC7
    pub(crate) pa7: Option<SensorConfig>,
    /// ADC8
    pub(crate) pb0: Option<SensorConfig>,
    /// ADC9
    pub(crate) pb1: Option<SensorConfig>,
}

pub(crate) struct SensorConfig {
    pub(crate) cond: TrigCond,
    /// Typical configuration is left = 0, right = 1, up = 2, down = 3
    pub(crate) btn: usize,
}

/// Trigger condition
#[derive(Clone, Debug)]
pub(crate) enum TrigCond {
    /// Sensor triggers when the ADC value is higher than this value
    Abs(u16),
    /// Sensor triggers when the ADC value is above `thr` fraction of the valid number range [idle..=max]
    Rel {
        /// Level at rest
        idle: u16,
        /// Trigger level fraction [0..=1]
        thr: f32,
    },
}

impl TrigCond {
    pub fn get_thr(&self) -> u16 {
        match self {
            TrigCond::Abs(thr) => *thr,
            TrigCond::Rel { idle, thr } => ((u16::MAX - idle) as f32 * thr) as u16 + idle,
        }
    }
}

impl Profile {
    pub fn get_adc_map(&self) -> Vec<(usize, u16), 10> {
        let mut map = Vec::new();

        unsafe {
            if let Some(c) = &self.pa0 {
                map.push((c.btn, c.cond.get_thr())).unwrap_unchecked();
            }
            if let Some(c) = &self.pa1 {
                map.push((c.btn, c.cond.get_thr())).unwrap_unchecked();
            }
            if let Some(c) = &self.pa2 {
                map.push((c.btn, c.cond.get_thr())).unwrap_unchecked();
            }
            if let Some(c) = &self.pa3 {
                map.push((c.btn, c.cond.get_thr())).unwrap_unchecked();
            }
            if let Some(c) = &self.pa4 {
                map.push((c.btn, c.cond.get_thr())).unwrap_unchecked();
            }
            if let Some(c) = &self.pa5 {
                map.push((c.btn, c.cond.get_thr())).unwrap_unchecked();
            }
            if let Some(c) = &self.pa6 {
                map.push((c.btn, c.cond.get_thr())).unwrap_unchecked();
            }
            if let Some(c) = &self.pa7 {
                map.push((c.btn, c.cond.get_thr())).unwrap_unchecked();
            }
            if let Some(c) = &self.pb0 {
                map.push((c.btn, c.cond.get_thr())).unwrap_unchecked();
            }
            if let Some(c) = &self.pb1 {
                map.push((c.btn, c.cond.get_thr())).unwrap_unchecked();
            }
        }

        map
    }

    //pub fn get_pin_map(&self,
}
