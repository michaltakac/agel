//! The x86-64 backend written in Agel, run hosted: the fib IR becomes a
//! static ELF for the Agel supervisor, and what it cannot compile is a
//! condition, not a guess.
use agel_core::{Budget, EvaluationOptions, Value, World};

fn world() -> (World, EvaluationOptions) {
    let mut world = World::new(0);
    let options = EvaluationOptions {
        budget: Budget {
            fuel: 50_000_000,
            ..Budget::default()
        },
        ..EvaluationOptions::default()
    };
    agel_stdlib::install(&mut world, &options).unwrap();
    world
        .evaluate_with("(import agel/native) (import agel/native-x86)", &options)
        .unwrap();
    (world, options)
}

fn emit(
    world: &mut World,
    options: &EvaluationOptions,
    program: &str,
    arguments: &str,
) -> Result<Vec<u8>, String> {
    let commit = world
        .evaluate_with(
            &format!("(native-x86-emit (native-compile '{program}) '{arguments})"),
            options,
        )
        .map_err(|error| error.to_string())?;
    let Some(Value::String(hex)) = commit.values.last() else {
        return Err("no text".into());
    };
    Ok(hex
        .split_whitespace()
        .flat_map(|line| {
            (0..line.len() / 2)
                .map(move |i| u8::from_str_radix(&line[2 * i..2 * i + 2], 16).unwrap())
        })
        .collect())
}

#[test]
fn fib_becomes_a_static_elf_for_the_process_window() {
    let (mut world, options) = world();
    let elf = emit(
        &mut world,
        &options,
        "(fn (self n) (if (< n 2) n (+ (self self (- n 1)) (self self (- n 2)))))",
        "(10)",
    )
    .unwrap();
    assert_eq!(&elf[..4], b"\x7fELF");
    assert_eq!(elf[4], 2, "64-bit");
    assert_eq!(u16::from_le_bytes([elf[16], elf[17]]), 2, "ET_EXEC");
    assert_eq!(u16::from_le_bytes([elf[18], elf[19]]), 0x3e, "x86-64");
    let entry = u64::from_le_bytes(elf[24..32].try_into().unwrap());
    assert_eq!(entry, 0x80_1000_0000 + 176);
    let filesz = u64::from_le_bytes(elf[64 + 32..64 + 40].try_into().unwrap());
    assert_eq!(filesz as usize, elf.len(), "the code segment is the file");
    let arena = u64::from_le_bytes(elf[120 + 16..120 + 24].try_into().unwrap());
    assert_eq!(arena, 0x80_1010_0000, "the arena a megabyte above");
    // The entry: mov r15, rdi. fib's self-calls are operands of `+`, not
    // tail calls, so fib itself compiles with an ordinary call+ret.
    assert_eq!(&elf[176..179], &[0x49, 0x89, 0xff]);
}

#[test]
fn a_tail_recursive_loop_reuses_its_frame() {
    let (mut world, options) = world();
    let elf = emit(
        &mut world,
        &options,
        "(fn (self n acc) (if (= n 0) acc (self self (- n 1) (+ acc n))))",
        "(10 0)",
    )
    .unwrap();
    // The self call in tail position reuses the frame: a jump through the
    // closure's code, not a call, and the callee pops its own block of
    // three words (two arguments and the closure) on return.
    assert!(elf.windows(3).any(|w| w == [0x41, 0xff, 0x23]), "jmp [r11]");
    // The callee pops its own block of four words on return: three
    // arguments and the closure, 8*(3+1) = 32. (The entry still makes one
    // ordinary call to enter the top-level closure; only the self call in
    // tail position is the jump.)
    assert!(elf.windows(3).any(|w| w == [0xc2, 0x20, 0x00]), "ret 32");
}

#[test]
fn an_inlined_lambda_in_an_operand_preserves_its_continuation() {
    let (mut world, options) = world();
    let elf = emit(
        &mut world,
        &options,
        "(fn (self n) (if (= n 0) 0 (+ (let ((m (- n 1))) (self self m)) 1)))",
        "(5)",
    )
    .unwrap();
    assert!(
        !elf.windows(3).any(|w| w == [0x41, 0xff, 0x23]),
        "the recursive call must return to the addition; the guest suite checks its result"
    );
}

#[test]
fn what_it_cannot_compile_is_refused() {
    let (mut world, options) = world();
    let error = emit(&mut world, &options, "(fn (x) (cons x nil))", "(1)").unwrap_err();
    assert!(error.contains("native-x86/unsupported"), "{error}");
    let error = emit(&mut world, &options, "(fn (x) x)", "(1 2)").unwrap_err();
    assert!(error.contains("arity"), "{error}");
}
