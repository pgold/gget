use thiserror::Error;

const CRLF: [u8; 2] = [0x0D, 0x0A];

#[derive(Debug, PartialEq)]
pub struct Header {
    pub status: String,
    pub meta: String,
}

#[derive(Debug, PartialEq)]
pub struct Response {
    pub header: Header,
    pub body: String,
}

#[derive(Error, Debug, PartialEq)]
pub enum GeminiError {
    #[error("header is too short")]
    HeaderTooShort,

    #[error("header is missing space character")]
    MissingSpaceCharacter,

    #[error("server response is missing CRLF")]
    MissingCRLF,

    #[error("unknown status returned ({0})")]
    UnknownStatus(String),

    #[error(transparent)]
    Utf8Error(#[from] std::str::Utf8Error),
}

#[derive(Debug)]
pub enum StatusCategory {
    Input,
    Success,
    Redirect,
    TemporaryFailure,
    PermanentFailure,
    ClientCertificateRequired,
}

pub fn status_category(status: &str) -> Result<StatusCategory, GeminiError> {
    match &status[0..1] {
        "1" => Ok(StatusCategory::Input),
        "2" => Ok(StatusCategory::Success),
        "3" => Ok(StatusCategory::Redirect),
        "4" => Ok(StatusCategory::TemporaryFailure),
        "5" => Ok(StatusCategory::PermanentFailure),
        "6" => Ok(StatusCategory::ClientCertificateRequired),
        _ => Err(GeminiError::UnknownStatus(status.to_string())),
    }
}

fn parse_header(header: &[u8]) -> Result<Header, GeminiError> {
    // Header must be at least 3 characters long (2 status code characters, and
    // 1 space).
    match header.len() {
        0..=2 => return Err(GeminiError::HeaderTooShort),
        _ => (),
    }

    // First two characters are the status code.
    let raw_status = &header[0..2];

    // One space character expected.
    match header[2] {
        0x20 => (),
        _ => return Err(GeminiError::MissingSpaceCharacter),
    }

    // Remainder of the header line is the meta field.
    let raw_meta = &header[3..];

    let status = std::str::from_utf8(&raw_status)?.to_string();
    let meta = std::str::from_utf8(&raw_meta)?.to_string();
    Ok(Header { status, meta })
}

pub fn parse_response(plaintext: &[u8]) -> Result<Response, GeminiError> {
    // Split by first CR LF sequence.
    let split_loc = match find_first(plaintext, &CRLF) {
        Some(v) => v,
        None => return Err(GeminiError::MissingCRLF),
    };
    let raw_header = &plaintext[0..split_loc];
    let raw_body = &plaintext[split_loc + CRLF.len()..];

    let header = parse_header(&raw_header)?;

    // It is assumed that the response body is UTF-8 decodable.
    // TODO(pgold): consider checking whether the header for the encoding and
    // act accordingly.
    let body = std::str::from_utf8(raw_body)?.to_string();
    Ok(Response { header, body })
}

pub fn request(url: &str) -> std::vec::Vec<u8> {
    [url.as_bytes(), &CRLF].concat()
}

fn find_first(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    // TODO(pgold): consider using a more efficient version (benchmark first).
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::parse_response;
    use super::GeminiError;

    #[test]
    fn parse_response_happy() -> Result<(), GeminiError> {
        parse_response("20 text/gemini\r\n".as_bytes())?;
        parse_response("20 text/gemini\r\n ".as_bytes())?;
        parse_response("20 text/gemini\r\nzzz".as_bytes())?;
        Ok(())
    }

    #[test]
    fn parse_response_error() {
        assert_eq!(
            parse_response("20 text/gemini".as_bytes()),
            Err(GeminiError::MissingCRLF)
        );
        assert_eq!(
            parse_response("\r\n".as_bytes()),
            Err(GeminiError::HeaderTooShort)
        );
        assert_eq!(
            parse_response("20\r\n".as_bytes()),
            Err(GeminiError::HeaderTooShort)
        );
        assert_eq!(
            parse_response("20text/gemini\r\n".as_bytes()),
            Err(GeminiError::MissingSpaceCharacter)
        );
    }
}
