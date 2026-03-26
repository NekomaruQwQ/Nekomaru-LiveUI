use tap::Pipe as _;

/// Serialize `value` as pretty-printed JSON with 4-space indentation.
pub fn to_json(value: &impl serde::Serialize) -> String {
    use serde_json::ser::{PrettyFormatter, Serializer};

    let fmt = PrettyFormatter::with_indent(b"    ");
    let mut buf = Vec::new();
    let mut ser = Serializer::with_formatter(&mut buf, fmt);

    value
        .serialize(&mut ser)
        .expect("failed to serialize `value` as JSON");
    buf
        .pipe(String::from_utf8)
        .expect("failed to serialize `value` as JSON: Invalid UTF-8")
}
