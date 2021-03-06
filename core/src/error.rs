use std::fmt::{Debug, Display, Formatter};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    UnimplementedOpcode { opcode: u8 },
    InvalidReadPort { port: u8 },
    InvalidWritePort { port: u8 },
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnimplementedOpcode { opcode } => write!(f, "unimplemented opcode: 0x{:02X}", opcode),
            Self::InvalidWritePort { port } => write!(f, "invalid write port: {}", port),
            Self::InvalidReadPort { port } => write!(f, "invalid read port: {}", port),
        }
    }
}

impl std::error::Error for Error {}
