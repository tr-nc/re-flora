use crate::config::OutputDevicePolicy;
use crate::error::{PetalSonicError, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use ringbuf::{HeapCons, traits::Consumer};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

const STARTUP_UNDERRUN_GRACE_CALLBACKS: usize = 8;
const OUTPUT_FADE_IN_MILLISECONDS: usize = 10;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct StereoFrame {
    pub(crate) left: f32,
    pub(crate) right: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OutputDeviceState {
    pub(crate) diagnostic_name: String,
    pub(crate) sample_rate: u32,
    pub(crate) physical_channels: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutputRecoveryReason {
    StreamFailure,
    SelectionChanged,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct OutputRecoveryRequest {
    pub(crate) probe: bool,
    pub(crate) retry_now: bool,
    pub(crate) elapsed_without_output: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OutputRecoveryResult {
    Stable,
    Running(OutputDeviceState),
    Recovering(OutputRecoveryCause),
    Failed(OutputFailure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutputRecoveryCause {
    DeviceUnavailable,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PreparedOutput {
    token: u64,
    pub(crate) device: OutputDeviceState,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum OutputPreparation {
    Ready(PreparedOutput),
    Unavailable,
    RequiresStop,
    Failed(OutputFailure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutputFailure {
    UnsupportedSampleFormat,
    PlatformLifecycle,
}

/// The internal seam between logical stereo rendering and physical output.
///
/// Implementations own device discovery, selection, negotiation, stream/callback
/// construction, physical channel mapping, and platform lifecycle. The engine can
/// only bind a pre-rendered logical-stereo consumer to an opaque prepared output.
/// `Ready` yields the sole capability for that prepared physical handle: the
/// owner must either move it into `open` or retain it until `shutdown` succeeds.
pub(crate) trait OutputPlatform {
    fn prepare(&mut self, policy: &OutputDevicePolicy) -> OutputPreparation;

    fn open(
        &mut self,
        prepared: PreparedOutput,
        callback: OutputCallback,
    ) -> Result<OutputDeviceState>;

    fn recovery_reason(
        &self,
        policy: &OutputDevicePolicy,
        active: &OutputDeviceState,
    ) -> Option<OutputRecoveryReason>;

    /// Stops the active callback/stream while preserving the one prepared
    /// replacement represented by the caller's linear `PreparedOutput`.
    fn stop_active_preserving_prepared(&mut self) -> Result<()>;

    /// Releases every active and prepared physical handle. A successful return
    /// is the only platform transition that permits the owner to become fully
    /// stopped and discard any remaining `PreparedOutput` capability.
    fn shutdown(&mut self) -> Result<()>;
}

pub(crate) struct OutputCallback {
    is_running: Arc<AtomicBool>,
    frames_processed: Arc<AtomicUsize>,
    underrun_count: Arc<AtomicUsize>,
    consumer: HeapCons<StereoFrame>,
    startup_underrun_callbacks_remaining: usize,
    fade_in_remaining_frames: usize,
    fade_in_total_frames: usize,
}

impl OutputCallback {
    pub(crate) fn new(
        is_running: Arc<AtomicBool>,
        frames_processed: Arc<AtomicUsize>,
        underrun_count: Arc<AtomicUsize>,
        consumer: HeapCons<StereoFrame>,
        sample_rate: u32,
    ) -> Self {
        let fade_frames = (sample_rate as usize * OUTPUT_FADE_IN_MILLISECONDS / 1000).max(1);
        Self {
            is_running,
            frames_processed,
            underrun_count,
            consumer,
            startup_underrun_callbacks_remaining: STARTUP_UNDERRUN_GRACE_CALLBACKS,
            fade_in_remaining_frames: fade_frames,
            fade_in_total_frames: fade_frames,
        }
    }

    fn write<T>(&mut self, data: &mut [T], physical_channels: u16)
    where
        T: SizedSample + FromSample<f32>,
    {
        let channels = physical_channels as usize;
        if channels == 0 || !self.is_running.load(Ordering::Relaxed) {
            data.fill(T::from_sample(0.0));
            return;
        }

        let device_frames = data.len() / channels;
        let mut consumed = 0;
        for frame_index in 0..device_frames {
            let Some(frame) = self.consumer.try_pop() else {
                if self.startup_underrun_callbacks_remaining > 0 {
                    self.startup_underrun_callbacks_remaining -= 1;
                } else {
                    self.underrun_count.fetch_add(1, Ordering::Relaxed);
                }
                data[frame_index * channels..].fill(T::from_sample(0.0));
                break;
            };
            let fade = if self.fade_in_remaining_frames > 0 {
                let completed = self
                    .fade_in_total_frames
                    .saturating_sub(self.fade_in_remaining_frames);
                self.fade_in_remaining_frames -= 1;
                completed as f32 / self.fade_in_total_frames.max(1) as f32
            } else {
                1.0
            };
            let start = frame_index * channels;
            if channels == 1 {
                data[start] = T::from_sample((frame.left + frame.right) * 0.5 * fade);
            } else {
                data[start] = T::from_sample(frame.left * fade);
                data[start + 1] = T::from_sample(frame.right * fade);
                data[start + 2..start + channels].fill(T::from_sample(0.0));
            }
            consumed += 1;
        }
        self.frames_processed.fetch_add(consumed, Ordering::Relaxed);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OutputDeviceInfo {
    name: String,
    is_default: bool,
    aliases: Vec<String>,
}

struct OutputDeviceCandidate {
    device: cpal::Device,
    info: OutputDeviceInfo,
}

struct PreparedCpalOutput {
    token: u64,
    device: cpal::Device,
    config: cpal::SupportedStreamConfig,
    stream_config: cpal::StreamConfig,
    probe: Option<cpal::Stream>,
}

pub(crate) struct CpalOutputPlatform {
    stream: Option<cpal::Stream>,
    prepared: Option<PreparedCpalOutput>,
    stream_error: Arc<AtomicBool>,
    next_token: u64,
    #[cfg(target_os = "windows")]
    _thread_apartment: crate::platform::OutputThreadApartment,
}

impl CpalOutputPlatform {
    pub(crate) fn new() -> Result<Self> {
        #[cfg(target_os = "windows")]
        crate::platform::ensure_audio_context()?;
        #[cfg(target_os = "windows")]
        let thread_apartment = crate::platform::initialize_output_thread()?;

        Ok(Self {
            stream: None,
            prepared: None,
            stream_error: Arc::new(AtomicBool::new(false)),
            next_token: 1,
            #[cfg(target_os = "windows")]
            _thread_apartment: thread_apartment,
        })
    }

    fn select_device(
        policy: &OutputDevicePolicy,
    ) -> Result<(cpal::Device, cpal::SupportedStreamConfig, OutputDeviceState)> {
        let host = cpal::default_host();
        let device = match policy {
            OutputDevicePolicy::FollowSystemDefault => {
                host.default_output_device().ok_or_else(|| {
                    PetalSonicError::AudioDevice("No default output device available".into())
                })?
            }
            OutputDevicePolicy::PinnedNameContains(requested) => {
                Self::find_output_device(&host, requested)?
            }
        };
        let name = device
            .name()
            .unwrap_or_else(|_| "Unknown output device".to_string());
        let config = device.default_output_config().map_err(|error| {
            PetalSonicError::AudioDevice(format!(
                "Failed to get default config for output device '{name}': {error}"
            ))
        })?;
        let state = OutputDeviceState {
            diagnostic_name: name,
            sample_rate: config.sample_rate().0,
            physical_channels: config.channels(),
        };
        Ok((device, config, state))
    }

    fn find_output_device(host: &cpal::Host, requested: &str) -> Result<cpal::Device> {
        let mut matches = Self::device_candidates(host)?
            .into_iter()
            .filter(|candidate| Self::device_matches(&candidate.info, requested))
            .collect::<Vec<_>>();
        if matches.is_empty() {
            let available = Self::device_candidates(host)?
                .into_iter()
                .map(|candidate| candidate.info)
                .collect::<Vec<_>>();
            return Err(PetalSonicError::AudioDevice(format!(
                "No output device name or alias matches '{requested}'. Available output devices: {}",
                Self::format_device_infos(&available)
            )));
        }
        let infos = matches
            .iter()
            .map(|candidate| candidate.info.clone())
            .collect::<Vec<_>>();
        if infos.len() > 1 {
            log::warn!(
                "PetalSonic output device substring '{requested}' matched multiple devices: {}; using {}",
                Self::format_device_infos(&infos),
                Self::format_device_info(&infos[0])
            );
        }
        Ok(matches.remove(0).device)
    }

    fn device_candidates(host: &cpal::Host) -> Result<Vec<OutputDeviceCandidate>> {
        let default_name = host
            .default_output_device()
            .and_then(|device| device.name().ok());
        let aliases = Self::alsa_card_aliases();
        let devices = host.output_devices().map_err(|error| {
            PetalSonicError::AudioDevice(format!("Failed to enumerate output devices: {error}"))
        })?;
        Ok(devices
            .map(|device| {
                let name = device
                    .name()
                    .unwrap_or_else(|_| "Unknown output device".to_string());
                OutputDeviceCandidate {
                    info: OutputDeviceInfo {
                        is_default: default_name.as_deref() == Some(name.as_str()),
                        aliases: Self::device_aliases(&name, &aliases),
                        name,
                    },
                    device,
                }
            })
            .collect())
    }

    fn device_matches(info: &OutputDeviceInfo, requested: &str) -> bool {
        let requested = Self::normalize_name(requested);
        !requested.is_empty()
            && (Self::normalize_name(&info.name).contains(&requested)
                || info.aliases.iter().any(|alias| {
                    let alias = Self::normalize_name(alias);
                    !alias.is_empty()
                        && (alias.contains(&requested)
                            || alias.len() >= 3 && requested.contains(&alias))
                }))
    }

    fn device_aliases(
        device_name: &str,
        aliases_by_card: &HashMap<String, Vec<String>>,
    ) -> Vec<String> {
        let mut aliases = Vec::new();
        for card in Self::alsa_card_ids(device_name) {
            if let Some(card_aliases) = aliases_by_card.get(&card) {
                aliases.extend(card_aliases.iter().cloned());
            }
        }
        Self::dedup_strings(&mut aliases);
        aliases
    }

    fn alsa_card_ids(device_name: &str) -> Vec<String> {
        let mut ids = Vec::new();
        let mut rest = device_name;
        while let Some(index) = rest.find("CARD=") {
            let after = &rest[index + "CARD=".len()..];
            let id = after
                .split(|ch: char| ch == ',' || ch == ':' || ch.is_whitespace())
                .next()
                .unwrap_or("")
                .trim();
            if !id.is_empty() {
                ids.push(id.to_string());
            }
            rest = &after[id.len()..];
        }
        Self::dedup_strings(&mut ids);
        ids
    }

    #[cfg(target_os = "linux")]
    fn alsa_card_aliases() -> HashMap<String, Vec<String>> {
        Self::parse_alsa_card_aliases(Path::new("/proc/asound/cards"))
    }

    #[cfg(not(target_os = "linux"))]
    fn alsa_card_aliases() -> HashMap<String, Vec<String>> {
        HashMap::new()
    }

    fn parse_alsa_card_aliases(path: &Path) -> HashMap<String, Vec<String>> {
        let Ok(contents) = std::fs::read_to_string(path) else {
            return HashMap::new();
        };
        let mut result = HashMap::new();
        let mut lines = contents.lines();
        while let Some(line) = lines.next() {
            let Some(open) = line.find('[') else { continue };
            let Some(close) = line[open + 1..].find(']').map(|offset| open + 1 + offset) else {
                continue;
            };
            let card = line[open + 1..close].trim();
            if card.is_empty() {
                continue;
            }
            let mut aliases = vec![card.to_string()];
            if let Some((_, display)) = line.split_once(" - ") {
                aliases.push(display.trim().to_string());
            }
            if let Some(detail_line) = lines.next() {
                let detail = detail_line
                    .trim()
                    .split_once(" at ")
                    .map(|(name, _)| name)
                    .unwrap_or_else(|| detail_line.trim());
                if !detail.is_empty() {
                    aliases.push(detail.to_string());
                }
            }
            Self::dedup_strings(&mut aliases);
            result.insert(card.to_string(), aliases);
        }
        result
    }

    fn dedup_strings(values: &mut Vec<String>) {
        let mut seen = Vec::new();
        values.retain(|value| {
            let normalized = Self::normalize_name(value);
            if normalized.is_empty() || seen.contains(&normalized) {
                false
            } else {
                seen.push(normalized);
                true
            }
        });
    }

    fn normalize_name(name: &str) -> String {
        name.trim().to_lowercase()
    }

    fn format_device_infos(devices: &[OutputDeviceInfo]) -> String {
        if devices.is_empty() {
            "<none>".to_string()
        } else {
            devices
                .iter()
                .map(Self::format_device_info)
                .collect::<Vec<_>>()
                .join(", ")
        }
    }

    fn format_device_info(device: &OutputDeviceInfo) -> String {
        let default = if device.is_default { " (default)" } else { "" };
        if device.aliases.is_empty() {
            format!("'{}'{default}", device.name)
        } else {
            format!(
                "'{}'{default} (aliases: {})",
                device.name,
                device
                    .aliases
                    .iter()
                    .map(|alias| format!("'{alias}'"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    }

    fn stream_config(
        state: &OutputDeviceState,
        buffer_size: cpal::BufferSize,
    ) -> cpal::StreamConfig {
        cpal::StreamConfig {
            channels: state.physical_channels,
            sample_rate: cpal::SampleRate(state.sample_rate),
            buffer_size,
        }
    }

    fn requested_buffer_size() -> Option<u32> {
        const ENV_KEY: &str = "PETALSONIC_BUFFER_SIZE";
        if let Ok(value) = std::env::var(ENV_KEY) {
            if let Ok(parsed) = value.parse::<u32>()
                && parsed > 0
            {
                return Some(parsed);
            }
            log::warn!("Invalid {ENV_KEY} value '{value}', falling back to platform defaults");
        }
        Self::platform_default_buffer_size()
    }

    fn buffer_size(config: &cpal::SupportedStreamConfig) -> cpal::BufferSize {
        Self::negotiate_buffer_size(Self::requested_buffer_size(), config.buffer_size())
    }

    fn negotiate_buffer_size(
        requested: Option<u32>,
        supported: &cpal::SupportedBufferSize,
    ) -> cpal::BufferSize {
        let Some(requested) = requested else {
            return cpal::BufferSize::Default;
        };
        let selected = match supported {
            cpal::SupportedBufferSize::Range { min, max } => requested.clamp(*min, *max),
            cpal::SupportedBufferSize::Unknown => requested,
        };
        cpal::BufferSize::Fixed(selected)
    }

    #[cfg(target_os = "linux")]
    fn platform_default_buffer_size() -> Option<u32> {
        Some(1024)
    }

    #[cfg(not(target_os = "linux"))]
    fn platform_default_buffer_size() -> Option<u32> {
        None
    }

    fn build_silent_probe<T>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
    ) -> Result<cpal::Stream>
    where
        T: SizedSample + FromSample<f32>,
    {
        device
            .build_output_stream(
                config,
                move |data: &mut [T], _| data.fill(T::from_sample(0.0)),
                move |_| {},
                None,
            )
            .map_err(|error| {
                PetalSonicError::AudioDevice(format!(
                    "Failed to open selected output device: {error}"
                ))
            })
    }

    fn build_probe_for_format(
        device: &cpal::Device,
        supported: &cpal::SupportedStreamConfig,
        config: &cpal::StreamConfig,
    ) -> Result<cpal::Stream> {
        match supported.sample_format() {
            cpal::SampleFormat::F32 => Self::build_silent_probe::<f32>(device, config),
            cpal::SampleFormat::I16 => Self::build_silent_probe::<i16>(device, config),
            cpal::SampleFormat::U16 => Self::build_silent_probe::<u16>(device, config),
            _ => Err(PetalSonicError::PermanentDeviceFailure(
                "Unsupported sample format".into(),
            )),
        }
    }

    fn build_stream<T>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        physical_channels: u16,
        mut callback: OutputCallback,
        stream_error: Arc<AtomicBool>,
    ) -> Result<cpal::Stream>
    where
        T: SizedSample + FromSample<f32>,
    {
        device
            .build_output_stream(
                config,
                move |data: &mut [T], _| callback.write(data, physical_channels),
                move |_| stream_error.store(true, Ordering::Release),
                None,
            )
            .map_err(|error| {
                PetalSonicError::AudioDevice(format!("Failed to build output stream: {error}"))
            })
    }

    fn build_for_format(
        device: &cpal::Device,
        supported: &cpal::SupportedStreamConfig,
        config: &cpal::StreamConfig,
        state: &OutputDeviceState,
        callback: OutputCallback,
        stream_error: Arc<AtomicBool>,
    ) -> Result<cpal::Stream> {
        match supported.sample_format() {
            cpal::SampleFormat::F32 => Self::build_stream::<f32>(
                device,
                config,
                state.physical_channels,
                callback,
                stream_error,
            ),
            cpal::SampleFormat::I16 => Self::build_stream::<i16>(
                device,
                config,
                state.physical_channels,
                callback,
                stream_error,
            ),
            cpal::SampleFormat::U16 => Self::build_stream::<u16>(
                device,
                config,
                state.physical_channels,
                callback,
                stream_error,
            ),
            _ => Err(PetalSonicError::PermanentDeviceFailure(
                "Unsupported sample format".into(),
            )),
        }
    }
}

impl OutputPlatform for CpalOutputPlatform {
    fn prepare(&mut self, policy: &OutputDevicePolicy) -> OutputPreparation {
        let Ok((device, config, state)) = Self::select_device(policy) else {
            return OutputPreparation::Unavailable;
        };
        if !matches!(
            config.sample_format(),
            cpal::SampleFormat::F32 | cpal::SampleFormat::I16 | cpal::SampleFormat::U16
        ) {
            return OutputPreparation::Failed(OutputFailure::UnsupportedSampleFormat);
        }
        let requested_config = Self::stream_config(&state, Self::buffer_size(&config));
        let default_config = Self::stream_config(&state, cpal::BufferSize::Default);
        let (probe, stream_config) =
            match Self::build_probe_for_format(&device, &config, &requested_config) {
                Ok(probe) => (Some(probe), requested_config),
                Err(_) if !matches!(requested_config.buffer_size, cpal::BufferSize::Default) => {
                    match Self::build_probe_for_format(&device, &config, &default_config) {
                        Ok(probe) => (Some(probe), default_config),
                        Err(_) if self.stream.is_some() => return OutputPreparation::RequiresStop,
                        Err(_) => return OutputPreparation::Unavailable,
                    }
                }
                Err(_) if self.stream.is_some() => return OutputPreparation::RequiresStop,
                Err(_) => return OutputPreparation::Unavailable,
            };
        let token = self.next_token;
        self.next_token = self.next_token.wrapping_add(1).max(1);
        self.prepared = Some(PreparedCpalOutput {
            token,
            device,
            config,
            stream_config,
            probe,
        });
        OutputPreparation::Ready(PreparedOutput {
            token,
            device: state,
        })
    }

    fn open(
        &mut self,
        prepared: PreparedOutput,
        callback: OutputCallback,
    ) -> Result<OutputDeviceState> {
        let Some(PreparedCpalOutput {
            token,
            device,
            config,
            stream_config,
            probe,
        }) = self.prepared.take()
        else {
            return Err(PetalSonicError::AudioDevice(
                "No output device has been prepared".into(),
            ));
        };
        if token != prepared.token {
            return Err(PetalSonicError::AudioDevice(
                "Prepared output capability is stale".into(),
            ));
        }
        drop(probe);
        self.stream_error.store(false, Ordering::Release);
        let stream = Self::build_for_format(
            &device,
            &config,
            &stream_config,
            &prepared.device,
            callback,
            self.stream_error.clone(),
        )?;
        stream.play().map_err(|error| {
            PetalSonicError::AudioDevice(format!("Failed to start output stream: {error}"))
        })?;
        self.stream = Some(stream);
        Ok(prepared.device)
    }

    fn recovery_reason(
        &self,
        policy: &OutputDevicePolicy,
        active: &OutputDeviceState,
    ) -> Option<OutputRecoveryReason> {
        if self.stream.is_none() || self.stream_error.load(Ordering::Acquire) {
            return Some(OutputRecoveryReason::StreamFailure);
        }
        if !matches!(policy, OutputDevicePolicy::FollowSystemDefault) {
            return None;
        }
        let default_name = cpal::default_host()
            .default_output_device()
            .and_then(|device| device.name().ok());
        (default_name.as_deref() != Some(active.diagnostic_name.as_str()))
            .then_some(OutputRecoveryReason::SelectionChanged)
    }

    fn stop_active_preserving_prepared(&mut self) -> Result<()> {
        drop(self.stream.take());
        Ok(())
    }

    fn shutdown(&mut self) -> Result<()> {
        drop(self.stream.take());
        drop(self.prepared.take());
        Ok(())
    }
}

impl Drop for CpalOutputPlatform {
    fn drop(&mut self) {
        drop(self.stream.take());
        drop(self.prepared.take());
    }
}

#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::sync::Mutex;
    use std::time::Duration;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum FakeSampleFormat {
        F32,
        I16,
        U16,
        Unsupported,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(crate) struct FakeDevice {
        pub(crate) state: OutputDeviceState,
        pub(crate) aliases: Vec<String>,
        pub(crate) sample_format: FakeSampleFormat,
        pub(crate) requires_exclusive_open: bool,
    }

    impl FakeDevice {
        pub(crate) fn stereo(name: &str, sample_rate: u32) -> Self {
            Self {
                state: OutputDeviceState {
                    diagnostic_name: name.into(),
                    sample_rate,
                    physical_channels: 2,
                },
                aliases: Vec::new(),
                sample_format: FakeSampleFormat::F32,
                requires_exclusive_open: false,
            }
        }
    }

    #[derive(Default)]
    struct FakeState {
        devices: Vec<FakeDevice>,
        selected: Option<usize>,
        active: Option<usize>,
        prepared: Option<(u64, usize)>,
        callback: Option<OutputCallback>,
        stream_failed: bool,
        fail_next_open: bool,
        stop_failures_remaining: usize,
        stop_fails: bool,
        captured: Vec<f32>,
        actions: Vec<&'static str>,
        virtual_time: Duration,
    }

    #[derive(Clone)]
    pub(crate) struct FakeOutputHandle(Arc<Mutex<FakeState>>);

    impl FakeOutputHandle {
        pub(crate) fn set_selected(&self, selected: Option<usize>) {
            self.0.lock().unwrap().selected = selected;
        }

        pub(crate) fn fail_stream(&self) {
            self.0.lock().unwrap().stream_failed = true;
        }

        pub(crate) fn fail_next_open(&self) {
            self.0.lock().unwrap().fail_next_open = true;
        }

        pub(crate) fn fail_next_stop(&self) {
            self.0.lock().unwrap().stop_failures_remaining += 1;
        }

        pub(crate) fn fail_all_stops(&self) {
            self.0.lock().unwrap().stop_fails = true;
        }

        pub(crate) fn allow_stops(&self) {
            self.0.lock().unwrap().stop_fails = false;
        }

        pub(crate) fn output_ownership(&self) -> (bool, bool) {
            let state = self.0.lock().unwrap();
            (
                state.active.is_some() || state.callback.is_some(),
                state.prepared.is_some(),
            )
        }

        pub(crate) fn advance(&self, frames: usize) {
            let mut state = self.0.lock().unwrap();
            let Some(active) = state.active else { return };
            let channels = state.devices[active].state.physical_channels;
            let sample_rate = state.devices[active].state.sample_rate;
            let mut output = vec![0.0f32; frames * channels as usize];
            if let Some(callback) = state.callback.as_mut() {
                callback.write(&mut output, channels);
            }
            state.captured.extend(output);
            state.virtual_time += Duration::from_secs_f64(frames as f64 / sample_rate as f64);
        }

        pub(crate) fn captured(&self) -> Vec<f32> {
            self.0.lock().unwrap().captured.clone()
        }

        pub(crate) fn actions(&self) -> Vec<&'static str> {
            self.0.lock().unwrap().actions.clone()
        }

        pub(crate) fn virtual_time(&self) -> Duration {
            self.0.lock().unwrap().virtual_time
        }
    }

    pub(crate) struct FakeOutputPlatform {
        state: Arc<Mutex<FakeState>>,
        next_token: u64,
    }

    impl FakeOutputPlatform {
        pub(crate) fn scripted(
            devices: Vec<FakeDevice>,
            selected: Option<usize>,
        ) -> (Self, FakeOutputHandle) {
            let state = Arc::new(Mutex::new(FakeState {
                devices,
                selected,
                ..FakeState::default()
            }));
            (
                Self {
                    state: state.clone(),
                    next_token: 1,
                },
                FakeOutputHandle(state),
            )
        }

        fn selected_device(state: &FakeState, policy: &OutputDevicePolicy) -> Option<usize> {
            match policy {
                OutputDevicePolicy::FollowSystemDefault => state.selected,
                OutputDevicePolicy::PinnedNameContains(requested) => {
                    state.devices.iter().position(|device| {
                        let info = OutputDeviceInfo {
                            name: device.state.diagnostic_name.clone(),
                            is_default: false,
                            aliases: device.aliases.clone(),
                        };
                        CpalOutputPlatform::device_matches(&info, requested)
                    })
                }
            }
        }
    }

    impl OutputPlatform for FakeOutputPlatform {
        fn prepare(&mut self, policy: &OutputDevicePolicy) -> OutputPreparation {
            let mut state = self.state.lock().unwrap();
            state.actions.push("prepare");
            let Some(selected) = Self::selected_device(&state, policy) else {
                return OutputPreparation::Unavailable;
            };
            if state.active.is_some() && state.devices[selected].requires_exclusive_open {
                return OutputPreparation::RequiresStop;
            }
            let token = self.next_token;
            self.next_token = self.next_token.wrapping_add(1).max(1);
            state.prepared = Some((token, selected));
            OutputPreparation::Ready(PreparedOutput {
                token,
                device: state.devices[selected].state.clone(),
            })
        }

        fn open(
            &mut self,
            prepared: PreparedOutput,
            callback: OutputCallback,
        ) -> Result<OutputDeviceState> {
            let mut state = self.state.lock().unwrap();
            state.actions.push("open");
            let Some((token, selected)) = state.prepared.take() else {
                return Err(PetalSonicError::AudioDevice(
                    "No fake output has been prepared".into(),
                ));
            };
            if token != prepared.token {
                return Err(PetalSonicError::AudioDevice(
                    "Prepared fake output capability is stale".into(),
                ));
            }
            if std::mem::take(&mut state.fail_next_open) {
                return Err(PetalSonicError::AudioDevice(
                    "scripted fake output open failure".into(),
                ));
            }
            if state.devices[selected].sample_format == FakeSampleFormat::Unsupported {
                return Err(PetalSonicError::PermanentDeviceFailure(
                    "unsupported fake sample format".into(),
                ));
            }
            state.active = Some(selected);
            state.callback = Some(callback);
            state.stream_failed = false;
            Ok(state.devices[selected].state.clone())
        }

        fn recovery_reason(
            &self,
            policy: &OutputDevicePolicy,
            _active: &OutputDeviceState,
        ) -> Option<OutputRecoveryReason> {
            let state = self.state.lock().unwrap();
            if state.stream_failed || state.active.is_none() {
                return Some(OutputRecoveryReason::StreamFailure);
            }
            (Self::selected_device(&state, policy) != state.active)
                .then_some(OutputRecoveryReason::SelectionChanged)
        }

        fn stop_active_preserving_prepared(&mut self) -> Result<()> {
            let mut state = self.state.lock().unwrap();
            state.actions.push("stop-active");
            if state.stop_fails || state.stop_failures_remaining > 0 {
                state.stop_failures_remaining = state.stop_failures_remaining.saturating_sub(1);
                return Err(PetalSonicError::AudioDevice(
                    "scripted fake output stop failure".into(),
                ));
            }
            state.callback = None;
            state.active = None;
            Ok(())
        }

        fn shutdown(&mut self) -> Result<()> {
            let mut state = self.state.lock().unwrap();
            state.actions.push("shutdown");
            if state.stop_fails || state.stop_failures_remaining > 0 {
                state.stop_failures_remaining = state.stop_failures_remaining.saturating_sub(1);
                return Err(PetalSonicError::AudioDevice(
                    "scripted fake output shutdown failure".into(),
                ));
            }
            state.callback = None;
            state.active = None;
            state.prepared = None;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::{FakeDevice, FakeOutputPlatform, FakeSampleFormat};
    use super::*;
    use ringbuf::{
        HeapRb,
        traits::{Producer, Split},
    };
    use std::time::Duration;

    fn callback_for(frames: &[StereoFrame], sample_rate: u32) -> OutputCallback {
        let ring = HeapRb::new(frames.len().max(1));
        let (mut producer, consumer) = ring.split();
        for frame in frames {
            producer.try_push(*frame).unwrap();
        }
        let mut callback = OutputCallback::new(
            Arc::new(AtomicBool::new(true)),
            Arc::new(AtomicUsize::new(0)),
            Arc::new(AtomicUsize::new(0)),
            consumer,
            sample_rate,
        );
        callback.fade_in_remaining_frames = 0;
        callback
    }

    #[test]
    fn callback_maps_logical_stereo_to_physical_channels() {
        let frames = [StereoFrame {
            left: 0.75,
            right: -0.25,
        }];
        let mut mono = callback_for(&frames, 48_000);
        let mut mono_output = [1.0f32; 1];
        mono.write(&mut mono_output, 1);
        assert_eq!(mono_output, [0.25]);

        let mut surround = callback_for(&frames, 48_000);
        let mut surround_output = [1.0f32; 6];
        surround.write(&mut surround_output, 6);
        assert_eq!(surround_output, [0.75, -0.25, 0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn callback_and_error_signal_are_allocation_free() {
        let frames = [StereoFrame {
            left: 0.5,
            right: -0.5,
        }; 4];
        let mut callback = callback_for(&frames, 48_000);
        let mut output = [0.0f32; 8];
        let stream_error = AtomicBool::new(false);

        let activity = crate::test_support::realtime_memory_activity(|| {
            callback.write(&mut output, 2);
            stream_error.store(true, Ordering::Release);
        });

        assert_eq!(activity, 0);
        assert_eq!(output, [0.5, -0.5, 0.5, -0.5, 0.5, -0.5, 0.5, -0.5]);
        assert!(stream_error.load(Ordering::Acquire));
    }

    #[test]
    fn callback_converts_supported_sample_formats_without_changing_mapping() {
        let frames = [StereoFrame {
            left: 0.5,
            right: -0.5,
        }];
        let mut i16_callback = callback_for(&frames, 48_000);
        let mut i16_output = [0i16; 2];
        i16_callback.write(&mut i16_output, 2);
        assert!(i16_output[0] > 0 && i16_output[1] < 0);

        let mut u16_callback = callback_for(&frames, 48_000);
        let mut u16_output = [0u16; 2];
        u16_callback.write(&mut u16_output, 2);
        assert!(u16_output[0] > u16::MAX / 2);
        assert!(u16_output[1] < u16::MAX / 2);
    }

    #[test]
    fn buffer_negotiation_clamps_fixed_requests_and_preserves_default() {
        let range = cpal::SupportedBufferSize::Range { min: 128, max: 512 };
        assert_eq!(
            CpalOutputPlatform::negotiate_buffer_size(Some(64), &range),
            cpal::BufferSize::Fixed(128)
        );
        assert_eq!(
            CpalOutputPlatform::negotiate_buffer_size(Some(1024), &range),
            cpal::BufferSize::Fixed(512)
        );
        assert_eq!(
            CpalOutputPlatform::negotiate_buffer_size(None, &range),
            cpal::BufferSize::Default
        );
    }

    #[test]
    fn device_matching_accepts_linux_alias_and_pipewire_style_name() {
        let device = OutputDeviceInfo {
            name: "sysdefault:CARD=KA3".into(),
            is_default: false,
            aliases: vec!["KA3".into(), "FiiO KA3".into()],
        };
        assert!(CpalOutputPlatform::device_matches(&device, "ka3"));
        assert!(CpalOutputPlatform::device_matches(
            &device,
            "FiiO KA3 Analog Stereo"
        ));
        assert!(!CpalOutputPlatform::device_matches(&device, "EX2050S"));
    }

    #[test]
    fn alsa_alias_parser_stays_inside_the_platform_adapter() {
        let path = std::env::temp_dir().join(format!(
            "petalsonic-output-adapter-cards-{}.txt",
            std::process::id()
        ));
        std::fs::write(
            &path,
            " 4 [KA3            ]: USB-Audio - FiiO KA3\n                      FiiO FiiO KA3 at usb-0000:00:14.0-11, high speed\n",
        )
        .unwrap();
        let aliases = CpalOutputPlatform::parse_alsa_card_aliases(&path);
        assert_eq!(
            aliases.get("KA3").unwrap(),
            &["KA3", "FiiO KA3", "FiiO FiiO KA3"]
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn fake_adapter_uses_virtual_time_and_recovers_physical_layout() {
        let a = FakeDevice::stereo("A", 48_000);
        let mut b = FakeDevice::stereo("B", 44_100);
        b.state.physical_channels = 6;
        b.sample_format = FakeSampleFormat::I16;
        let (mut adapter, handle) = FakeOutputPlatform::scripted(vec![a, b], Some(0));
        let policy = OutputDevicePolicy::FollowSystemDefault;
        let OutputPreparation::Ready(prepared_a) = adapter.prepare(&policy) else {
            panic!("A must prepare");
        };
        let callback = callback_for(
            &[StereoFrame {
                left: 0.75,
                right: -0.25,
            }],
            48_000,
        );
        let active_a = adapter.open(prepared_a, callback).unwrap();
        handle.advance(1);
        assert_eq!(handle.captured(), [0.75, -0.25]);
        assert_eq!(
            handle.virtual_time(),
            Duration::from_secs_f64(1.0 / 48_000.0)
        );

        handle.set_selected(Some(1));
        assert_eq!(
            adapter.recovery_reason(&policy, &active_a),
            Some(OutputRecoveryReason::SelectionChanged)
        );
        let OutputPreparation::Ready(prepared_b) = adapter.prepare(&policy) else {
            panic!("B must prepare before A is stopped");
        };
        adapter.stop_active_preserving_prepared().unwrap();
        let callback = callback_for(
            &[StereoFrame {
                left: 0.25,
                right: -0.5,
            }],
            44_100,
        );
        let active_b = adapter.open(prepared_b, callback).unwrap();
        handle.advance(1);
        assert_eq!(active_b.physical_channels, 6);
        assert_eq!(&handle.captured()[2..], &[0.25, -0.5, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(
            handle.actions(),
            ["prepare", "open", "prepare", "stop-active", "open"]
        );
    }

    #[test]
    fn fake_adapter_keeps_healthy_output_when_new_selection_is_unavailable() {
        let a = FakeDevice::stereo("A", 48_000);
        let (mut adapter, handle) = FakeOutputPlatform::scripted(vec![a], Some(0));
        let policy = OutputDevicePolicy::FollowSystemDefault;
        let OutputPreparation::Ready(prepared) = adapter.prepare(&policy) else {
            panic!("A must prepare");
        };
        let active = adapter.open(prepared, callback_for(&[], 48_000)).unwrap();
        handle.set_selected(None);
        assert_eq!(
            adapter.recovery_reason(&policy, &active),
            Some(OutputRecoveryReason::SelectionChanged)
        );
        assert_eq!(adapter.prepare(&policy), OutputPreparation::Unavailable);
        assert_eq!(
            adapter.recovery_reason(&OutputDevicePolicy::PinnedNameContains("A".into()), &active),
            None
        );
    }

    #[test]
    fn fake_shutdown_discards_an_unopened_prepared_handle() {
        let device = FakeDevice::stereo("A", 48_000);
        let (mut adapter, handle) = FakeOutputPlatform::scripted(vec![device], Some(0));
        let OutputPreparation::Ready(prepared) =
            adapter.prepare(&OutputDevicePolicy::FollowSystemDefault)
        else {
            panic!("A must prepare");
        };

        assert_eq!(handle.output_ownership(), (false, true));
        adapter.shutdown().unwrap();
        assert_eq!(handle.output_ownership(), (false, false));
        assert_eq!(handle.actions(), ["prepare", "shutdown"]);

        drop(prepared);
    }

    #[test]
    fn fake_adapter_reports_exclusive_stop_then_rebuild_fallback() {
        let a = FakeDevice::stereo("A", 48_000);
        let mut b = FakeDevice::stereo("B", 44_100);
        b.requires_exclusive_open = true;
        let (mut adapter, handle) = FakeOutputPlatform::scripted(vec![a, b], Some(0));
        let policy = OutputDevicePolicy::FollowSystemDefault;
        let OutputPreparation::Ready(prepared_a) = adapter.prepare(&policy) else {
            panic!("A must prepare");
        };
        adapter.open(prepared_a, callback_for(&[], 48_000)).unwrap();
        handle.set_selected(Some(1));
        assert_eq!(adapter.prepare(&policy), OutputPreparation::RequiresStop);
        adapter.shutdown().unwrap();
        let OutputPreparation::Ready(prepared_b) = adapter.prepare(&policy) else {
            panic!("B must prepare after A stops");
        };
        adapter.open(prepared_b, callback_for(&[], 44_100)).unwrap();
        assert_eq!(
            handle.actions(),
            ["prepare", "open", "prepare", "shutdown", "prepare", "open"]
        );
    }

    #[test]
    fn fake_adapter_stop_quiesces_callback_before_returning() {
        let device = FakeDevice::stereo("A", 48_000);
        let (mut adapter, handle) = FakeOutputPlatform::scripted(vec![device], Some(0));
        let OutputPreparation::Ready(prepared) =
            adapter.prepare(&OutputDevicePolicy::FollowSystemDefault)
        else {
            panic!("A must prepare");
        };
        adapter
            .open(
                prepared,
                callback_for(
                    &[StereoFrame {
                        left: 1.0,
                        right: -1.0,
                    }],
                    48_000,
                ),
            )
            .unwrap();
        adapter.shutdown().unwrap();
        handle.advance(1);
        assert!(handle.captured().is_empty());
        assert_eq!(handle.actions(), ["prepare", "open", "shutdown"]);
    }

    #[test]
    fn fake_adapter_classifies_supported_and_permanent_formats() {
        for format in [
            FakeSampleFormat::F32,
            FakeSampleFormat::I16,
            FakeSampleFormat::U16,
        ] {
            let mut device = FakeDevice::stereo("supported", 48_000);
            device.sample_format = format;
            let (mut adapter, _) = FakeOutputPlatform::scripted(vec![device], Some(0));
            let OutputPreparation::Ready(prepared) =
                adapter.prepare(&OutputDevicePolicy::FollowSystemDefault)
            else {
                panic!("supported fake format must prepare");
            };
            adapter.open(prepared, callback_for(&[], 48_000)).unwrap();
            adapter.shutdown().unwrap();
        }

        let mut unsupported = FakeDevice::stereo("unsupported", 48_000);
        unsupported.sample_format = FakeSampleFormat::Unsupported;
        let (mut adapter, _) = FakeOutputPlatform::scripted(vec![unsupported], Some(0));
        let OutputPreparation::Ready(prepared) =
            adapter.prepare(&OutputDevicePolicy::FollowSystemDefault)
        else {
            panic!("selection remains independent from format support");
        };
        assert!(matches!(
            adapter.open(prepared, callback_for(&[], 48_000)),
            Err(PetalSonicError::PermanentDeviceFailure(_))
        ));
    }
}
