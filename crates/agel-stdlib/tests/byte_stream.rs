use agel_core::{EvaluationOptions, Value, World};
#[test]
fn byte_adapters_bound_and_validate_the_transport() {
    exercise_stream(false);
    exercise_stream(true);
}

fn exercise_stream(agel_service: bool) {
    let mut world = World::default();
    let options = EvaluationOptions::default();
    agel_stdlib::install(&mut world, &options).unwrap();
    for source in [
        include_str!("../bytes.agel"),
        include_str!("../byte-stream.agel"),
        include_str!("../stream-buffer.agel"),
        include_str!("../buffered-stream.agel"),
        include_str!("../services.agel"),
        include_str!("../service-stream.agel"),
    ] {
        world.evaluate(source).unwrap();
    }
    if agel_service {
        world.evaluate("(import agel/service-stream) (def service-buffer (fn (stream size capabilities) (service-stream stream size)))").unwrap();
    }
    let source = if agel_service {
        include_str!("byte-stream.agel")
            .replace(
                "(import agel/buffered-stream)",
                "(import agel/service-stream)",
            )
            .replace("(buffered-stream ", "(service-buffer ")
    } else {
        include_str!("byte-stream.agel").to_owned()
    };
    assert_eq!(
        world.evaluate(&source).unwrap().values.pop(),
        Some(Value::Bool(true))
    );
    if agel_service {
        assert!(
            world.events().is_empty(),
            "Agel services must not create bootstrap actor heap snapshots"
        );
    }
}

#[test]
fn agel_services_preserve_nested_updates_and_require_explicit_fault_recovery() {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &Default::default()).unwrap();
    world.evaluate(include_str!("../services.agel")).unwrap();
    let result = world.evaluate(r#"(import agel/services)
      (def nested (service-new "nested" (fn (state n) (dict 'state (+ state n) 'reply (+ state n))) 10))
      (def outer (service-new "outer" (fn (state n) (let ((reply (service-call nested n))) (dict 'state (+ state 1) 'reply reply))) 0))
      (service-call outer 5)
      (def recursive (service-new "recursive" (fn (state message)
        (if (= message 'reenter) (service-call recursive 'add) (dict 'state (+ state 1) 'reply (+ state 1)))) 0))
      (def error (service-call recursive 'reenter))
      (list (get (service-info nested) 'state) (get (service-info outer) 'state)
        (get (service-info recursive) 'state) (get (get error 'error) 'message))"#).unwrap();
    assert_eq!(
        result.values.last().unwrap().to_string(),
        "(15 1 0 \"reentrant service call\")"
    );
    assert!(world.evaluate("(service-call recursive 'add)").is_err());
    assert_eq!(world.evaluate("(service-restart recursive) (service-call recursive 'add) (list (get (service-info recursive) 'state) (get (service-info recursive) 'calls) (get (service-info recursive) 'busy))").unwrap().values.last().unwrap().to_string(), "(1 2 #f)");
    assert!(world.events().is_empty());
}

#[test]
fn packed_large_observations_fit_a_bounded_encoding_budget() {
    let mut world = World::default();
    let options = EvaluationOptions {
        budget: agel_core::Budget {
            fuel: 2_000_000,
            ..Default::default()
        },
        ..Default::default()
    };
    agel_stdlib::install(&mut world, &options).unwrap();
    world
        .evaluate_with(include_str!("../bytes.agel"), &options)
        .unwrap();
    let source = format!(
        r#"(import agel/bytes)
      (def record (dict 'bytes '({})))
      (def service-calls 0)
      (def packed (bytes-pack-field-serviced record 'bytes
        (fn () (def service-calls (+ service-calls 1)))))
      (bytes-hex-serviced '(1 2 3) (fn () (signal 'test/service "small encodes need no service")))
      (list (= (get (get packed 'bytes) 'data) {})
            (= (bytes-unpack-field packed 'bytes) record) service-calls)"#,
        std::iter::repeat_n("165", 4096)
            .collect::<Vec<_>>()
            .join(" "),
        Value::String("a5".repeat(4096))
    );
    assert_eq!(
        world
            .evaluate_with(&source, &options)
            .unwrap()
            .values
            .last()
            .unwrap()
            .to_string(),
        "(#t #t 64)"
    );
}

#[test]
fn profiling_separates_service_time_and_calibrates_a_conservative_limit() {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &Default::default()).unwrap();
    for source in [
        include_str!("../services.agel"),
        include_str!("../work-profile.agel"),
    ] {
        world.evaluate(source).unwrap();
    }
    let result = world.evaluate(r#"(import agel/work-profile)
      (def t 0)
      (def work (profile-work
        (fn (pulse) (begin (def t (+ t 10)) (pulse) (def t (+ t 20)) (pulse) (def t (+ t 30)) 'done))
        (fn () (def t (+ t 100))) (fn () t)))
      (def stats (get work 'metrics))
      (list (get work 'value) (get stats 'calls) (get stats 'elapsed-us) (get stats 'service-us)
        (get stats 'work-max-us) (get stats 'gap-max-us) (profile-limit stats 64 300 2 512))"#).unwrap();
    assert_eq!(
        result.values.last().unwrap().to_string(),
        "(done 2 260 200 30 120 320)"
    );
    assert!(world
        .evaluate("(profile-limit (dict 'calls 1 'work-max-us 0) 64 300 2 512)")
        .is_err());
    assert!(world
        .evaluate(
            "(profile-work (fn (pulse) (pulse)) (fn () #t) (fn () (begin (def t (- t 1)) t)))"
        )
        .is_err());
}

#[test]
fn a_larger_quantum_preserves_bytes_and_validates_before_service() {
    let mut world = World::default();
    let options = EvaluationOptions {
        budget: agel_core::Budget {
            fuel: 2_000_000,
            ..Default::default()
        },
        ..Default::default()
    };
    agel_stdlib::install(&mut world, &options).unwrap();
    world
        .evaluate_with(include_str!("../bytes.agel"), &options)
        .unwrap();
    let result = world.evaluate_with(r#"(import agel/bytes)
      (def sample-bytes (fn (n out) (if (= n 0) out
        (sample-bytes (- n 1) (cons (- n (* 256 (/ n 256))) out)))))
      (def sample (sample-bytes 769 nil))
      (def pulses 0)
      (def service (fn () (def pulses (+ pulses 1))))
      (def before (bytes-hex-quantum sample service 64))
      (def old-count pulses) (def pulses 0)
      (def after (bytes-hex-quantum sample service 256))
      (def new-count pulses) (def pulses 0)
      (def invalid (with-handler (* e) (get e 'kind) (bytes-hex-quantum sample service 255)))
      (list (= before after) (= (bytes-from-hex after) sample) old-count new-count pulses invalid)"#, &options).unwrap();
    assert_eq!(
        result.values.last().unwrap().to_string(),
        "(#t #t 13 4 0 bytes/quantum)"
    );
}
