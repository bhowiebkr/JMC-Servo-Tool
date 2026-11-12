use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    /// The serial port could not be opened or configured.
    Serial(String),
    /// No complete reply before the deadline.
    Timeout { request: String, received: usize },
    /// The drive answered with a Modbus exception.
    Exception { request: String, code: u8 },
    /// A reply that parsed but did not match the request.
    BadReply(String),
    UnknownParam(String),
    /// A value that cannot be converted or is out of range.
    Value(String),
    /// Blocked by the tool's own safety rules.
    Refused(String),
    Xml(String),
    Json(String),
    /// The parameter table file is malformed or inconsistent.
    Table(String),
}

pub fn exception_meaning(code: u8) -> &'static str {
    match code {
        1 => "illegal function",
        2 => "illegal data address",
        3 => "illegal data value",
        4 => "slave device failure",
        5 => "acknowledge",
        6 => "slave device busy",
        _ => "unknown exception",
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {e}"),
            Error::Serial(s) => write!(f, "serial port: {s}"),
            Error::Timeout { request, received } => {
                write!(f, "no reply to {request} ({received} bytes received)")
            }
            Error::Exception { request, code } => write!(
                f,
                "drive refused {request}: exception {code:02} ({})",
                exception_meaning(*code)
            ),
            Error::BadReply(s) => write!(f, "bad reply: {s}"),
            Error::UnknownParam(s) => write!(f, "unknown parameter {s}"),
            Error::Value(s) => write!(f, "value: {s}"),
            Error::Refused(s) => write!(f, "refused: {s}"),
            Error::Xml(s) => write!(f, "XML: {s}"),
            Error::Json(s) => write!(f, "JSON: {s}"),
            Error::Table(s) => write!(f, "parameter table: {s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e.to_string())
    }
}
