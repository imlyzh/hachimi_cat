use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum Error {
    #[error("input device init error")]
    InputDeviceInitError,
    #[error("output device init error")]
    OutputDeviceInitError,
    #[error("unsupported sample format")]
    UnsupportedSampleFormat,
}
