use super::*;

#[test]
fn display_reader() {
    assert_eq!(Role::Reader.to_string(), "reader");
}

#[test]
fn display_writer() {
    assert_eq!(Role::Writer.to_string(), "writer");
}

#[test]
fn from_str_reader() {
    assert_eq!("reader".parse::<Role>().unwrap(), Role::Reader);
}

#[test]
fn from_str_writer() {
    assert_eq!("writer".parse::<Role>().unwrap(), Role::Writer);
}

#[test]
fn from_str_invalid() {
    let err = "admin".parse::<Role>();
    assert!(err.is_err());
    assert_eq!(err.unwrap_err().to_string(), "invalid role value");
}

#[test]
fn from_str_case_sensitive() {
    assert!("Reader".parse::<Role>().is_err());
    assert!("WRITER".parse::<Role>().is_err());
}

#[test]
fn can_write() {
    assert!(!Role::Reader.can_write());
    assert!(Role::Writer.can_write());
}

#[test]
fn serde_roundtrip() {
    let reader_json = serde_json::to_string(&Role::Reader).unwrap();
    assert_eq!(reader_json, r#""reader""#);

    let writer_json = serde_json::to_string(&Role::Writer).unwrap();
    assert_eq!(writer_json, r#""writer""#);

    let reader: Role = serde_json::from_str(&reader_json).unwrap();
    assert_eq!(reader, Role::Reader);

    let writer: Role = serde_json::from_str(&writer_json).unwrap();
    assert_eq!(writer, Role::Writer);
}
