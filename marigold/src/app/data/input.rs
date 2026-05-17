use std::collections::HashSet;

use bevy_ecs::{
    message::MessageReader,
    resource::Resource,
    system::{Commands, ResMut},
};
use winit::{event::MouseButton, keyboard::KeyCode};

use crate::app::messages::{KeyInputMessage, MouseInputMessage};

#[derive(Resource)]
pub struct Input {
    pub keys: ButtonInputs<KeyCode>,
    pub mouse_buttons: ButtonInputs<MouseButton>,
}

impl Input {
    fn new() -> Self {
        Self {
            keys: ButtonInputs::new(),
            mouse_buttons: ButtonInputs::new(),
        }
    }

    fn tick(&mut self) {
        self.keys.tick();
        self.mouse_buttons.tick();
    }

    pub fn init(mut commands: Commands) {
        commands.insert_resource(Input::new());
        log::info!("initialized input system");
    }

    pub fn update(mut input: ResMut<Input>) {
        input.tick();
    }
}

pub struct ButtonInputs<T>
where
    T: Copy + Eq + std::hash::Hash,
{
    pressed: HashSet<T>,
    just_pressed: HashSet<T>,
    just_released: HashSet<T>,
}

impl<T> ButtonInputs<T>
where
    T: Copy + Eq + std::hash::Hash,
{
    pub fn new() -> Self {
        Self {
            pressed: HashSet::new(),
            just_pressed: HashSet::new(),
            just_released: HashSet::new(),
        }
    }

    pub fn press(&mut self, input: T) {
        if self.pressed.insert(input) {
            self.just_pressed.insert(input);
        }
    }

    pub fn release(&mut self, input: T) {
        if self.pressed.remove(&input) {
            self.just_released.insert(input);
        }
    }

    pub fn tick(&mut self) {
        self.just_pressed.clear();
        self.just_released.clear();
    }

    pub fn pressed(&self, input: T) -> bool {
        self.pressed.contains(&input)
    }

    pub fn just_pressed(&self, input: T) -> bool {
        self.just_pressed.contains(&input)
    }

    #[expect(unused)]
    pub fn just_released(&self, input: T) -> bool {
        self.just_released.contains(&input)
    }
}

pub fn handle_keyboard_input_event(
    mut input: ResMut<Input>,
    mut key_events: MessageReader<KeyInputMessage>,
) {
    for event in key_events.read() {
        let key = match event.physical_key {
            winit::keyboard::PhysicalKey::Code(key_code) => key_code,
            winit::keyboard::PhysicalKey::Unidentified(native_key_code) => {
                log::warn!("Unidentified physical key press: {:?}", native_key_code);
                return;
            }
        };

        match event.state {
            winit::event::ElementState::Pressed => input.keys.press(key),
            winit::event::ElementState::Released => input.keys.release(key),
        }
    }
}

pub fn handle_mouse_input_event(
    mut input: ResMut<Input>,
    mut mouse_input_events: MessageReader<MouseInputMessage>,
) {
    for event in mouse_input_events.read() {
        match event.state {
            winit::event::ElementState::Pressed => input.mouse_buttons.press(event.button),
            winit::event::ElementState::Released => input.mouse_buttons.release(event.button),
        }
    }
}
