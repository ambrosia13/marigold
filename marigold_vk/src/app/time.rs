use std::time::{Duration, Instant};

use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Commands, ResMut};
use bevy_ecs::world::World;
use derived_deref::{Deref, DerefMut};

pub const NUM_SAMPLES: usize = 64;

#[derive(Resource, Debug)]
pub struct Time {
    last_frame: Instant,
    delta: Duration,
    frame_count: u128,
}

impl Time {
    fn new() -> Self {
        Self {
            last_frame: Instant::now(),
            delta: Duration::ZERO,
            frame_count: 0,
        }
    }

    pub fn tick(&mut self) {
        let new_instant = Instant::now();
        let delta = self.last_frame.elapsed();

        self.last_frame = new_instant;
        self.delta = delta;
        self.frame_count += 1;
    }

    pub fn delta(&self) -> Duration {
        self.delta
    }

    pub fn frame_count(&self) -> u128 {
        self.frame_count
    }

    pub fn init(mut commands: Commands) {
        commands.insert_resource(Time::new());
        log::info!("initialized time system");
    }
}

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

pub struct TimeCounter {
    samples: [Duration; NUM_SAMPLES],
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
            samples: [Default::default(); NUM_SAMPLES],
            index: Default::default(),
            count: Default::default(),
        }
    }
}
