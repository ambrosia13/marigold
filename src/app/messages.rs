use bevy_ecs::{
    message::{Message, Messages},
    system::ResMut,
    world::World,
};
use derived_deref::{Deref, DerefMut};

pub fn init_message_type<T: Message>(world: &mut World) {
    world.insert_resource(Messages::<T>::default());
}

pub fn update_message_type<T: Message>(mut messages: ResMut<Messages<T>>) {
    messages.update();
}

#[derive(Message, Deref, DerefMut)]
pub struct MouseMotionMessage(pub glam::DVec2);

#[derive(Message, Deref, DerefMut)]
pub struct KeyInputMessage(pub winit::event::KeyEvent);

#[derive(Message)]
pub struct MouseInputMessage {
    pub state: winit::event::ElementState,
    pub button: winit::event::MouseButton,
}
