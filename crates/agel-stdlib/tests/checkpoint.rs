use agel_core::{Budget, EvaluationOptions, HostError, HostWord, Value, World};
fn render(args: &[Value]) -> Result<Value, HostError> {
    Ok(Value::String(args[0].to_string()))
}
const HOST: &[HostWord] = &[HostWord {
    name: "render",
    capability: "",
    call: render,
}];
#[test]
fn inert_codec_and_checkpoint_chain_recover_only_complete_valid_commits() {
    let mut world = World::default();
    world.install_host(HOST);
    let options = EvaluationOptions {
        host: HOST,
        budget: Budget {
            fuel: 50_000_000,
            ..Default::default()
        },
        ..Default::default()
    };
    agel_stdlib::install(&mut world, &options).unwrap();
    for source in [
        include_str!("../bytes.agel"),
        include_str!("../data-codec.agel"),
        include_str!("../checkpoint.agel"),
        include_str!("../live-runtime.agel"),
    ] {
        world.evaluate_with(source, &options).unwrap();
    }
    assert_eq!(
        world
            .evaluate_with(include_str!("checkpoint.agel"), &options)
            .unwrap()
            .values
            .pop(),
        Some(Value::Bool(true))
    );
}

static LOG: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
fn append_log(args: &[Value]) -> Result<Value, HostError> {
    let [Value::String(path), Value::String(text)] = args else {
        panic!("append arguments")
    };
    assert_eq!(path, "judge-log");
    LOG.lock().unwrap().push_str(text);
    Ok(Value::Int(text.len() as i64))
}
fn read_log(args: &[Value]) -> Result<Value, HostError> {
    assert_eq!(args, &[Value::String("judge-log".into())]);
    Ok(Value::String(LOG.lock().unwrap().clone()))
}
fn console_log(_: &[Value]) -> Result<Value, HostError> {
    Ok(Value::Nil)
}
const STORAGE: &[HostWord] = &[
    HostWord {
        name: "print-form",
        capability: "",
        call: render,
    },
    HostWord {
        name: "file-append",
        capability: "",
        call: append_log,
    },
    HostWord {
        name: "file-read",
        capability: "",
        call: read_log,
    },
    HostWord {
        name: "console-log",
        capability: "",
        call: console_log,
    },
];

#[test]
fn recovery_scenario_fits_each_native_controller_budget() {
    LOG.lock().unwrap().clear();
    let options = EvaluationOptions {
        host: STORAGE,
        budget: Budget {
            fuel: 50_000_000,
            ..Default::default()
        },
        ..Default::default()
    };
    for phase in [
        include_str!("../../../examples/native-checkpoint-write.agel"),
        include_str!("../../../examples/native-checkpoint-recover.agel"),
        include_str!("../../../examples/native-checkpoint-revert.agel"),
    ] {
        let mut world = World::default();
        world.install_host(STORAGE);
        agel_stdlib::install(&mut world, &options).unwrap();
        let source = [
            include_str!("../bytes.agel"),
            include_str!("../data-codec.agel"),
            include_str!("../checkpoint.agel"),
            include_str!("../live-runtime.agel"),
            include_str!("../../../examples/native-checkpoint-common.agel"),
            phase,
        ]
        .join("\n");
        let commit = world.evaluate_with(&source, &options).unwrap();
        eprintln!("checkpoint phase fuel: {}", commit.steps_used);
        assert_eq!(commit.values.last(), Some(&Value::Bool(true)));
    }
}
