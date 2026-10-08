pub(crate) mod output;

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
pub(crate) use windows::{OutputThreadApartment, ensure_audio_context, initialize_output_thread};
