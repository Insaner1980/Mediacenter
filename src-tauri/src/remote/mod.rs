pub mod adapters;
pub mod protocol;
mod protocol_json;
mod protocol_methods;
pub mod store;

pub mod event_hub;
pub mod operations;
#[cfg(test)]
mod protocol_tests;
pub mod state;

pub mod auth;
pub mod dispatch;
pub mod limits;
pub mod pairing;
pub mod server;
pub mod tls;

pub mod discovery;
pub mod network;
pub mod setup;

pub mod browser_bridge;
pub mod browser_input;
pub mod browser_protocol;
pub mod browser_runtime;

pub mod media;

pub mod renderer;

pub mod folders;

pub mod drives;
pub mod settings_admin;
pub mod vpn;

pub mod autostart;
pub mod idle;
pub mod lifecycle;

#[cfg(test)]
mod interop_tests;
