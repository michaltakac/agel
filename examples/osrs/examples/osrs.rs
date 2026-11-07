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
    static ENDPOINT: RefCell<Option<SocketAddr>> = const { RefCell::new(None) };
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

fn open(arguments: &[Value]) -> Result<Value, HostError> {
    if !arguments.is_empty() {
        return Err(error("wire-open takes no arguments"));
    }
    STREAM.with_borrow_mut(|slot| {
        if slot.is_some() {
            return Err(error("stream is already open"));
        }
        let endpoint = ENDPOINT
            .with_borrow(|value| *value)
            .ok_or_else(|| error("missing endpoint"))?;
        let stream =
            TcpStream::connect_timeout(&endpoint, Duration::from_secs(5)).map_err(error)?;
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(error)?;
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .map_err(error)?;
        stream.set_nodelay(true).map_err(error)?;
        *slot = Some(stream);
        Ok(Value::Bool(true))
    })
}

fn close(arguments: &[Value]) -> Result<Value, HostError> {
    if !arguments.is_empty() {
        return Err(error("wire-close takes no arguments"));
    }
    STREAM.with_borrow_mut(|slot| *slot = None);
    Ok(Value::Bool(true))
}

fn entropy(arguments: &[Value]) -> Result<Value, HostError> {
    let [Value::Int(count)] = arguments else {
        return Err(error("entropy-read takes one byte count"));
    };
    if !(1..=256).contains(count) {
        return Err(error("entropy read exceeds byte bound"));
    }
    let mut bytes = vec![0; *count as usize];
    fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .map_err(error)?;
    Ok(Value::List(
        bytes
            .into_iter()
            .map(|b| Value::Int(i64::from(b)))
            .collect(),
    ))
}

fn console_log(arguments: &[Value]) -> Result<Value, HostError> {
    match arguments {
        [Value::String(message)] => eprintln!("agel-progress: {message}"),
        [value] => eprintln!("agel-progress: {value}"),
        _ => return Err(error("console-log takes one argument")),
    }
    Ok(Value::Nil)
}

static HOST: [HostWord; 6] = [
    HostWord {
        name: "console-log",
        capability: "console/write",
        call: console_log,
    },
    HostWord {
        name: "entropy-read",
        capability: "entropy/read",
        call: entropy,
    },
    HostWord {
        name: "wire-open",
        capability: "network/stream",
        call: open,
    },
    HostWord {
        name: "wire-close",
        capability: "network/stream",
        call: close,
    },
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
    ENDPOINT.with_borrow_mut(|slot| *slot = Some(SocketAddr::from((Ipv4Addr::LOCALHOST, port))));
    let mut world = World::default();
    world.install_host(&HOST);
    let options = EvaluationOptions {
        budget: Budget {
            fuel: 500_000_000,
            ..Budget::default()
        },
        capabilities: vec![
            world.issue_capability("network/stream", "*")?,
            world.issue_capability("entropy/read", "*")?,
            world.issue_capability("console/write", "*")?,
        ],
        host: &HOST,
        ..EvaluationOptions::default()
    };
    agel_stdlib::install(&mut world, &options)?;
    world.evaluate_with(
        include_str!("../../../crates/agel-stdlib/bytes.agel"),
        &options,
    )?;
    world.evaluate_with(include_str!("../osrs-wire.agel"), &options)?;
    world.evaluate_with(
        include_str!("../../../crates/agel-stdlib/crypto.agel"),
        &options,
    )?;
    world.evaluate_with(include_str!("../osrs-crypto.agel"), &options)?;
    world.evaluate_with(include_str!("../osrs-login.agel"), &options)?;
    world.evaluate_with(include_str!("../osrs-packets.agel"), &options)?;
    world.evaluate_with(include_str!("../osrs-bits.agel"), &options)?;
    world.evaluate_with(include_str!("../osrs-npcs.agel"), &options)?;
    world.evaluate_with(include_str!("../osrs-game.agel"), &options)?;
    if let Some(value) = world
        .evaluate_with(&fs::read_to_string(program)?, &options)?
        .values
        .last()
    {
        println!("{value}");
    }
    Ok(())
}
