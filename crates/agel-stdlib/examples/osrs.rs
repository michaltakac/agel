use agel_core::{Budget, EvaluationOptions, HostError, HostWord, Value, World};
use std::{
    cell::RefCell,
    env, fs,
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpStream},
    time::Duration,
};

thread_local! {
    static STREAM: RefCell<Option<TcpStream>> = const { RefCell::new(None) };
}

fn error(message: impl ToString) -> HostError {
    HostError {
        kind: "wire/io".into(),
        message: message.to_string(),
    }
}

fn write(arguments: &[Value]) -> Result<Value, HostError> {
    let [value] = arguments else {
        return Err(error("wire-write takes one byte list"));
    };
    let values = match value {
        Value::List(values) if values.len() <= 65536 => values.as_slice(),
        Value::Nil => &[],
        _ => return Err(error("byte list exceeds transport bound")),
    };
    let bytes = values
        .iter()
        .map(|value| match value {
            Value::Int(byte) => u8::try_from(*byte).map_err(error),
            _ => Err(error("wire-write needs bytes")),
        })
        .collect::<Result<Vec<_>, _>>()?;
    STREAM.with_borrow_mut(|stream| {
        stream
            .as_mut()
            .ok_or_else(|| error("closed stream"))?
            .write_all(&bytes)
            .map_err(error)
    })?;
    Ok(Value::Int(bytes.len() as i64))
}

fn read(arguments: &[Value]) -> Result<Value, HostError> {
    let [Value::Int(count)] = arguments else {
        return Err(error("wire-read takes one byte count"));
    };
    if !(1..=65536).contains(count) {
        return Err(error("read exceeds transport bound"));
    }
    let mut bytes = vec![0; *count as usize];
    let count = STREAM.with_borrow_mut(|stream| {
        stream
            .as_mut()
            .ok_or_else(|| error("closed stream"))?
            .read(&mut bytes)
            .map_err(error)
    })?;
    bytes.truncate(count);
    Ok(if bytes.is_empty() {
        Value::Nil
    } else {
        Value::List(
            bytes
                .into_iter()
                .map(|byte| Value::Int(i64::from(byte)))
                .collect(),
        )
    })
}

static HOST: [HostWord; 2] = [
    HostWord {
        name: "wire-read",
        capability: "network/stream",
        call: read,
    },
    HostWord {
        name: "wire-write",
        capability: "network/stream",
        call: write,
    },
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let port: u16 = args
        .next()
        .ok_or("usage: osrs PORT PROGRAM.agel")?
        .parse()?;
    let program = args.next().ok_or("missing Agel program")?;
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let stream = TcpStream::connect_timeout(
        &SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        Duration::from_secs(5),
    )?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    stream.set_nodelay(true)?;
    STREAM.with_borrow_mut(|slot| *slot = Some(stream));
    let mut world = World::default();
    world.install_host(&HOST);
    let options = EvaluationOptions {
        budget: Budget {
            fuel: 50_000_000,
            ..Budget::default()
        },
        capabilities: vec![world.issue_capability("network/stream", "*")?],
        host: &HOST,
        ..EvaluationOptions::default()
    };
    agel_stdlib::install(&mut world, &options)?;
    world.evaluate_with(include_str!("../osrs-wire.agel"), &options)?;
    for value in world
        .evaluate_with(&fs::read_to_string(program)?, &options)?
        .values
    {
        println!("{value}");
    }
    Ok(())
}
