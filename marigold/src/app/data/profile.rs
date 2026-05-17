use std::time::Duration;

use bevy_ecs::{resource::Resource, world::World};
use derived_deref::{Deref, DerefMut};

pub const FPS_NUM_SAMPLES: usize = 64;

#[derive(Resource, Deref, DerefMut, Default)]
pub struct FpsCounter(pub TimeCounter);

impl FpsCounter {
    pub fn average_fps(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }

        let sum: Duration = self.samples.iter().take(self.count).sum();
        let average_frametime = sum / self.count as u32;

        1.0 / average_frametime.as_secs_f64()
    }

    pub fn init(world: &mut World) {
        world.insert_resource(FpsCounter::default());
    }
}

#[derive(Resource, Deref, DerefMut, Default)]
pub struct GeometryPassFrametimes(pub TimeCounter);

impl GeometryPassFrametimes {
    pub fn init(world: &mut World) {
        world.insert_resource(GeometryPassFrametimes::default());
    }
}

pub struct TimeCounter {
    samples: [Duration; FPS_NUM_SAMPLES],
    index: usize,
    count: usize,
}

impl TimeCounter {
    fn push(&mut self, duration: Duration) {
        self.samples[self.index] = duration;
        self.index = (self.index + 1) % self.samples.len();
        self.count = (self.count + 1).min(self.samples.len());
    }

    pub fn tick(&mut self, delta: Duration) {
        self.push(delta);
    }

    pub fn samples(&self) -> &[Duration] {
        &self.samples
    }
}

impl Default for TimeCounter {
    fn default() -> Self {
        Self {
            samples: [Default::default(); FPS_NUM_SAMPLES],
            index: Default::default(),
            count: Default::default(),
        }
    }
}
