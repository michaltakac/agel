use std::env;
use std::fs;
use std::path::PathBuf;

const RECORD_BYTES: usize = 64;

fn main() {
    println!("cargo:rerun-if-changed=linker/x86_64.ld");
    println!("cargo:rerun-if-changed=linker/aarch64.ld");
    println!("cargo:rerun-if-changed=linker/riscv64.ld");
    println!("cargo:rerun-if-changed=../desktop/native-desktop.agel");
    compile_native_desktop();
}

fn compile_native_desktop() {
    let source = fs::read_to_string("../desktop/native-desktop.agel")
        .expect("read native Agel desktop vector source");
    let tokens = tokenize(&source);
    let mut cursor = 0;
    expect(&tokens, &mut cursor, "native-vector-frame");
    let width = number(&tokens, &mut cursor);
    let height = number(&tokens, &mut cursor);
    assert!(
        width > 0 && height > 0,
        "native vector viewport must be positive"
    );
    let mut records = Vec::<[u8; RECORD_BYTES]>::new();
    while cursor < tokens.len() {
        let operation = take(&tokens, &mut cursor);
        let mut record = [0_u8; RECORD_BYTES];
        match operation {
            "background-gradient" => {
                put(&mut record, 0, 1);
                put(&mut record, 1, color(&tokens, &mut cursor));
                put(&mut record, 2, color(&tokens, &mut cursor));
            }
            "rounded-rectangle" => {
                put(&mut record, 0, 2);
                for field in 1..=5 {
                    put(&mut record, field, number(&tokens, &mut cursor));
                }
                put(&mut record, 6, color(&tokens, &mut cursor));
            }
            "gradient-rounded-rectangle" => {
                put(&mut record, 0, 3);
                for field in 1..=5 {
                    put(&mut record, field, number(&tokens, &mut cursor));
                }
                put(&mut record, 6, color(&tokens, &mut cursor));
                put(&mut record, 7, color(&tokens, &mut cursor));
            }
            "ellipse" => {
                put(&mut record, 0, 4);
                for field in 1..=4 {
                    put(&mut record, field, number(&tokens, &mut cursor));
                }
                put(&mut record, 5, color(&tokens, &mut cursor));
            }
            "text" => {
                put(&mut record, 0, 5);
                put(&mut record, 1, number(&tokens, &mut cursor));
                put(&mut record, 2, number(&tokens, &mut cursor));
                put(&mut record, 3, number(&tokens, &mut cursor));
                put(&mut record, 4, color(&tokens, &mut cursor));
                let text = take(&tokens, &mut cursor).as_bytes();
                assert!(text.len() <= 28, "native vector text exceeds 28 bytes");
                assert!(
                    text.iter().all(u8::is_ascii),
                    "native vector text must be ASCII"
                );
                put(&mut record, 8, text.len() as u32);
                record[36..36 + text.len()].copy_from_slice(text);
            }
            unknown => panic!("unknown native vector operation: {unknown}"),
        }
        records.push(record);
    }
    assert!(
        !records.is_empty(),
        "native vector frame must contain commands"
    );
    assert!(
        records.len() <= 256,
        "native vector command budget exceeded"
    );

    let mut output = Vec::with_capacity(16 + records.len() * RECORD_BYTES);
    output.extend_from_slice(b"AGV1");
    output.extend_from_slice(&width.to_le_bytes());
    output.extend_from_slice(&height.to_le_bytes());
    output.extend_from_slice(&(records.len() as u32).to_le_bytes());
    for record in records {
        output.extend_from_slice(&record);
    }
    let path = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR")).join("native-desktop.agv");
    fs::write(path, output).expect("write compiled native vector stream");
}

fn tokenize(source: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut comment = false;
    let mut depth = 0_u32;
    for character in source.chars() {
        if comment {
            if character == '\n' {
                comment = false;
            }
        } else if quoted {
            if character == '"' {
                tokens.push(core::mem::take(&mut current));
                quoted = false;
            } else {
                current.push(character);
            }
        } else {
            match character {
                ';' => comment = true,
                '"' => {
                    flush(&mut tokens, &mut current);
                    quoted = true;
                }
                '(' => {
                    flush(&mut tokens, &mut current);
                    depth = depth
                        .checked_add(1)
                        .expect("native vector nesting overflow");
                }
                ')' => {
                    flush(&mut tokens, &mut current);
                    depth = depth
                        .checked_sub(1)
                        .expect("unexpected closing parenthesis");
                }
                ' ' | '\t' | '\r' | '\n' => flush(&mut tokens, &mut current),
                other => current.push(other),
            }
        }
    }
    assert!(!quoted, "unterminated string in native vector source");
    assert_eq!(depth, 0, "unclosed native vector form");
    flush(&mut tokens, &mut current);
    tokens
}

fn flush(tokens: &mut Vec<String>, current: &mut String) {
    if !current.is_empty() {
        tokens.push(core::mem::take(current));
    }
}

fn take<'a>(tokens: &'a [String], cursor: &mut usize) -> &'a str {
    let token = tokens.get(*cursor).expect("truncated native vector source");
    *cursor += 1;
    token
}

fn expect(tokens: &[String], cursor: &mut usize, expected: &str) {
    assert_eq!(
        take(tokens, cursor),
        expected,
        "unexpected native vector form"
    );
}

fn number(tokens: &[String], cursor: &mut usize) -> u32 {
    take(tokens, cursor)
        .parse()
        .expect("native vector coordinate must be a non-negative integer")
}

fn color(tokens: &[String], cursor: &mut usize) -> u32 {
    let value = take(tokens, cursor);
    assert!(
        value.len() == 7 && value.starts_with('#'),
        "color must be #RRGGBB"
    );
    u32::from_str_radix(&value[1..], 16).expect("color must be #RRGGBB")
}

fn put(record: &mut [u8; RECORD_BYTES], word: usize, value: u32) {
    let offset = word * 4;
    record[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
