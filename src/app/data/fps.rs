use std::time::Duration;

use bevy_ecs::{resource::Resource, world::World};

use crate::app::data::time::Time;

pub const FPS_NUM_SAMPLES: usize = 64;

#[derive(Resource)]
pub struct FpsCounter {
    samples: [Duration; FPS_NUM_SAMPLES],
    index: usize,
    count: usize,
}

impl FpsCounter {
    fn push(&mut self, duration: Duration) {
        self.samples[self.index] = duration;
        self.index = (self.index + 1) % self.samples.len();
        self.count = (self.count + 1).min(self.samples.len());
    }

    pub fn tick(&mut self, delta: Duration) {
        self.push(delta);
    }

    pub fn average_fps(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }

        let sum: Duration = self.samples.iter().take(self.count).sum();
        let average_frametime = sum / self.count as u32;

        1.0 / average_frametime.as_secs_f64()
    }

    pub fn low_fps(&self) -> f64 {
        if self.count <= 1 {
            return 0.0;
        }

        let mut samples = self.samples;

        let one_percent = ((self.count as f64 / 100.0) as usize).max(1);

        let (lows, _threshold, _highs) = samples.select_nth_unstable(one_percent);
        lows.iter().sum::<Duration>().as_secs_f64() / one_percent as f64
    }

    pub fn samples(&self) -> &[Duration] {
        &self.samples[..self.count]
    }

    pub fn init(world: &mut World) {
        world.insert_resource(FpsCounter::default());
    }
}

impl Default for FpsCounter {
    fn default() -> Self {
        Self {
            samples: [Default::default(); FPS_NUM_SAMPLES],
            index: Default::default(),
            count: Default::default(),
        }
    }
}
