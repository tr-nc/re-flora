use crate::error::{PetalSonicError, Result};
use cpal::traits::HostTrait;
use std::sync::OnceLock;
use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};

static WASAPI_CONTEXT: OnceLock<std::result::Result<(), String>> = OnceLock::new();

pub(crate) struct OutputThreadApartment {
    initialized: bool,
}

pub(crate) fn initialize_output_thread() -> Result<OutputThreadApartment> {
    OutputThreadApartment::initialize()
}

impl OutputThreadApartment {
    fn initialize() -> Result<Self> {
        let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if result.is_ok() {
            Ok(Self { initialized: true })
        } else if result == RPC_E_CHANGED_MODE {
            // Another subsystem already selected an apartment model for this
            // thread. COM is initialized and remains usable; this call must not
            // be balanced with CoUninitialize.
            Ok(Self { initialized: false })
        } else {
            Err(PetalSonicError::BackendUnavailable {
                backend: "WASAPI",
                reason: format!("failed to initialize COM on output supervisor: {result:?}"),
            })
        }
    }
}

impl Drop for OutputThreadApartment {
    fn drop(&mut self) {
        if self.initialized {
            unsafe { CoUninitialize() };
        }
    }
}

pub(crate) fn ensure_audio_context() -> Result<()> {
    let initialized = WASAPI_CONTEXT.get_or_init(|| {
        let (ready_sender, ready_receiver) = crossbeam_channel::bounded(1);
        std::thread::Builder::new()
            .name("petalsonic-wasapi-context".into())
            .spawn(move || {
                let apartment = match OutputThreadApartment::initialize() {
                    Ok(apartment) => apartment,
                    Err(error) => {
                        let _ = ready_sender.send(Err(error.to_string()));
                        return;
                    }
                };

                // CPAL 0.15 keeps its IMMDeviceEnumerator in a process-global
                // OnceLock even though COM apartments are thread-scoped. Create
                // that singleton on a process-lifetime thread so later worlds do
                // not inherit an enumerator from a supervisor that has exited.
                let _ = cpal::default_host().default_output_device();
                if ready_sender.send(Ok(())).is_err() {
                    return;
                }

                let _apartment = apartment;
                loop {
                    std::thread::park();
                }
            })
            .map_err(|error| format!("failed to start WASAPI context thread: {error}"))?;

        ready_receiver
            .recv()
            .map_err(|_| "WASAPI context thread exited during initialization".to_string())?
    });

    initialized
        .clone()
        .map_err(|reason| PetalSonicError::BackendUnavailable {
            backend: "WASAPI",
            reason,
        })
}
