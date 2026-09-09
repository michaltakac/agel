use agel_core::{EvaluationOptions, Value, World};
use agel_jit::managed::{Fault, Limits, Native};

#[path = "../examples/text_workshop.rs"]
mod workshop;

#[test]
fn text_workshop_example_is_executable_documentation() {
    assert_eq!(
        workshop::run(workshop::PROGRAM).unwrap(),
        Value::Map(vec![
            (Value::Symbol("answer".into()), Value::Int(3628800)),
            (
                Value::Symbol("message".into()),
                Value::String("Ahoj z Agelu 👋".into())
            ),
        ])
    );
}

fn bootstrap() -> (World, Native, Native) {
    let mut world = World::default();
    let mut options = EvaluationOptions::default();
    options.budget.fuel = 20_000_000;
    agel_stdlib::install(&mut world, &options).unwrap();
    world
        .evaluate("(import agel/native) (import agel/native-reader)")
        .unwrap();
    let ir = world
        .evaluate_with("(native-compile native-compiler-source)", &options)
        .unwrap()
        .values
        .pop()
        .unwrap();
    let compiler = Native::compile(&ir).unwrap();
    let source = world
        .evaluate("native-reader-source")
        .unwrap()
        .values
        .pop()
        .unwrap();
    let reader_ir = compiler.invoke(&[source], Limits::default()).unwrap().value;
    let reader = Native::compile(&reader_ir).unwrap();
    (world, compiler, reader)
}

fn read(reader: &Native, text: &str) -> Result<Value, Fault> {
    reader
        .invoke(
            &[
                Value::String(text.into()),
                Value::Int(65536),
                Value::Int(64),
            ],
            Limits::default(),
        )
        .map(|o| o.value)
}

#[test]
fn native_reader_matches_seed_for_syntax_unicode_numbers_and_errors() {
    let (mut world, _, reader) = bootstrap();
    for text in [
        "",
        "; comment",
        "() nil #t #f",
        "'(one (two 42))",
        "\"Ahoj 👋\" žluťoučký",
        "\"a\\n\\r\\t\\\"\\\\b\"",
        "0 -0 +1 -9223372036854775808 9223372036854775807",
        "9223372036854775808 -9223372036854775809 + - 1a",
        "foo'bar a\"b ;hi\n(next)",
    ] {
        let expected = world
            .evaluate(&format!("'( {text}\n )"))
            .unwrap()
            .values
            .pop()
            .unwrap();
        assert_eq!(read(&reader, text).unwrap(), expected, "{text}");
        let seed = world
            .evaluate(&format!(
                "(native-read {} 65536 64)",
                Value::String(text.into())
            ))
            .unwrap()
            .values
            .pop()
            .unwrap();
        assert_eq!(seed, expected, "seed reader: {text}");
    }
    for text in ["(", ")", "'", "\"unfinished", "\"\\q\"", "(a))"] {
        assert_eq!(read(&reader, text), Err(Fault::Signaled), "{text}");
    }
    assert_eq!(
        reader
            .invoke(
                &[Value::String("()".into()), Value::Int(2), Value::Int(0)],
                Limits::default()
            )
            .unwrap_err(),
        Fault::Signaled
    );
    assert_eq!(
        reader
            .invoke(
                &[Value::String("abc".into()), Value::Int(2), Value::Int(64)],
                Limits::default()
            )
            .unwrap_err(),
        Fault::Signaled
    );
    assert_eq!(
        reader
            .invoke(
                &[Value::String("abc".into()), Value::Int(3), Value::Int(64)],
                Limits {
                    fuel: 0,
                    ..Limits::default()
                }
            )
            .unwrap_err(),
        Fault::Fuel
    );
}

#[test]
fn reader_reads_itself_and_compiler_from_text_without_seed_roundtrip() {
    let (mut world, compiler, reader) = bootstrap();
    for (text, name) in [
        (agel_stdlib::NATIVE_READER, "native-reader-source"),
        (agel_stdlib::NATIVE_COMPILER, "native-compiler-source"),
    ] {
        let expected = world.evaluate(name).unwrap().values.pop().unwrap();
        let Value::List(forms) = read(&reader, text).unwrap() else {
            panic!("forms")
        };
        assert_eq!(forms, vec![expected]);
        let ir = compiler.invoke(&forms, Limits::default()).unwrap().value;
        if name == "native-reader-source" {
            assert_eq!(&ir, reader.ir());
            let next = Native::compile(&ir).unwrap();
            assert_eq!(
                read(&next, "'(hello 42)").unwrap(),
                read(&reader, "'(hello 42)").unwrap()
            );
        } else {
            assert_eq!(&ir, compiler.ir());
        }
    }
    let Value::List(forms) = read(&reader, "(fn (n) (+ n 1))").unwrap() else {
        panic!("forms")
    };
    let ir = compiler.invoke(&forms, Limits::default()).unwrap().value;
    assert_eq!(
        Native::compile(&ir)
            .unwrap()
            .invoke(&[Value::Int(41)], Limits::default())
            .unwrap()
            .value,
        Value::Int(42)
    );
}

#[test]
fn text_primitives_validate_boundaries_and_meter_copies() {
    let (mut world, compiler, _) = bootstrap();
    for (source, expected) in [
        ("(fn () (text-bytes \"👋\"))", Value::Int(4)),
        ("(fn () (text-byte \"👋\" 0))", Value::Int(240)),
        (
            "(fn () (text-slice \"a👋b\" 1 5))",
            Value::String("👋".into()),
        ),
        (
            "(fn () (text-symbol (text-concat \"ž\" \"aba\")))",
            Value::Symbol("žaba".into()),
        ),
    ] {
        let ast = world
            .evaluate(&format!("'{source}"))
            .unwrap()
            .values
            .pop()
            .unwrap();
        let ir = compiler.invoke(&[ast], Limits::default()).unwrap().value;
        let native = Native::compile(&ir).unwrap();
        assert_eq!(
            native.invoke(&[], Limits::default()).unwrap().value,
            expected
        );
        assert_eq!(
            world
                .evaluate(&format!("({source})"))
                .unwrap()
                .values
                .pop()
                .unwrap(),
            expected
        );
    }
    for expr in [
        "(text-slice \"👋\" 1 4)",
        "(text-byte \"a\" -1)",
        "(text-byte \"a\" 1)",
        "(text-slice \"abc\" 2 1)",
        "(text-concat \"a\" 1)",
        "(text-symbol 1)",
        "(text-byte \"a\")",
    ] {
        assert!(world.evaluate(expr).is_err());
        let ast = world
            .evaluate(&format!("'(fn () {expr})"))
            .unwrap()
            .values
            .pop()
            .unwrap();
        let ir = compiler.invoke(&[ast], Limits::default()).unwrap().value;
        assert!(Native::compile(&ir)
            .unwrap()
            .invoke(&[], Limits::default())
            .is_err());
    }
    let ast = world
        .evaluate("'(fn (s) (text-concat s s))")
        .unwrap()
        .values
        .pop()
        .unwrap();
    let ir = compiler.invoke(&[ast], Limits::default()).unwrap().value;
    let native = Native::compile(&ir).unwrap();
    assert_eq!(
        native
            .invoke(
                &[Value::String("abcd".into())],
                Limits {
                    text_bytes: 8,
                    ..Limits::default()
                }
            )
            .unwrap_err(),
        Fault::Heap
    );
    assert_eq!(
        native
            .invoke(&[Value::String("abcd".into())], Limits::default())
            .unwrap()
            .value,
        Value::String("abcdabcd".into())
    );
}
