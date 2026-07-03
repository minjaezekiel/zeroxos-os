//! Power manager — frequency scaling, sleep, hibernate, gaming-aware policy.

pub struct PowerManager {
    pub on_battery: bool,
    pub battery_pct: Option<u8>,
    pub gaming_mode: bool,
    pub thermal_throttled: bool,
    pub cpu_freq_khz: Vec<u32>,
}

impl PowerManager {
    pub fn new(cpu_count: usize) -> Self {
        Self {
            on_battery: false,
            battery_pct: None,
            gaming_mode: false,
            thermal_throttled: false,
            cpu_freq_khz: vec![3_000_000; cpu_count],
        }
    }

    /// Apply the gaming power policy: pin all cores to max frequency.
    pub fn enable_gaming_mode(&mut self) {
        self.gaming_mode = true;
        for cpu in 0..self.cpu_freq_khz.len() {
            self.cpu_freq_khz[cpu] = 3_000_000;
            hal::power::set_cpu_frequency(cpu as u32, 3_000_000);
        }
        log::info!("[pwr] gaming mode ON — all cores pinned to max frequency");
    }

    pub fn disable_gaming_mode(&mut self) {
        self.gaming_mode = false;
        log::info!("[pwr] gaming mode OFF — adaptive frequency scaling resumed");
    }

    /// Called by the AI power policy 30 times per second.
    pub fn ai_tune(&mut self, cpu: u32, target_freq_khz: u32) {
        if let Some(slot) = self.cpu_freq_khz.get_mut(cpu as usize) {
            *slot = target_freq_khz;
            hal::power::set_cpu_frequency(cpu, target_freq_khz);
        }
    }

    pub fn sleep(&self) {
        log::info!("[pwr] entering sleep state");
        unsafe { hal::power::sleep(hal::power::SleepState::Suspend); }
    }
}
