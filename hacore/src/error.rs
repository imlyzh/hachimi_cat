use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum Error {
    #[error("input device init error")]
    InputDeviceInitError,
    #[error("output device init error")]
    OutputDeviceInitError,
    #[error("unsupported input sample format")]
    UnsupportedInputSampleFormat,
    #[error("unsupported output sample format")]
    UnsupportedOutputSampleFormat,
}
