use agel_core::{EvaluationOptions, Value, World};
use agel_jit::managed::{Limits, Native};
#[cfg(not(test))]
use std::io::{self, Read};

pub const PACKAGE: &str = include_str!("../../../examples/jit-module-dock.agel");
pub struct Tools {
    pub reader: Native,
    pub compiler: Native,
    pub linker: Native,
}
impl Tools {
    pub fn bootstrap() -> Result<Self, Box<dyn std::error::Error>> {
        let mut world = World::default();
        let mut options = EvaluationOptions::default();
        options.budget.fuel = 20_000_000;
        agel_stdlib::install(&mut world, &options)?;
        world.evaluate(
            "(import agel/native) (import agel/native-reader) (import agel/native-modules)",
        )?;
        let ir = world
            .evaluate_with("(native-compile native-compiler-source)", &options)?
            .values
            .pop()
            .unwrap();
        let compiler = Native::compile(&ir)?;
        let reader = world
            .evaluate("native-reader-source")?
            .values
            .pop()
            .unwrap();
        let reader = Native::compile(&compiler.invoke(&[reader], Limits::default())?.value)?;
        drop(world);
        let forms = reader
            .invoke(
                &[
                    Value::String(agel_stdlib::NATIVE_MODULES.into()),
                    Value::Int(65536),
                    Value::Int(64),
                ],
                Limits {
                    fuel: 20_000_000,
                    ..Limits::default()
                },
            )?
            .value;
        let Value::List(forms) = forms else {
            return Err("expected linker function".into());
        };
        let linker = Native::compile(&compiler.invoke(&forms, Limits::default())?.value)?;
        Ok(Self {
            reader,
            compiler,
            linker,
        })
    }
    pub fn read(&self, text: &str) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
        let forms = self
            .reader
            .invoke(
                &[
                    Value::String(text.into()),
                    Value::Int(65536),
                    Value::Int(64),
                ],
                Limits::default(),
            )?
            .value;
        match forms {
            Value::Nil => Ok(vec![]),
            Value::List(xs) => Ok(xs),
            _ => Err("expected forms".into()),
        }
    }
    pub fn link(
        &self,
        text: &str,
        module: &str,
        name: &str,
    ) -> Result<(Value, Native), Box<dyn std::error::Error>> {
        let source = self
            .linker
            .invoke(
                &[
                    Value::List(self.read(text)?),
                    Value::Symbol(module.into()),
                    Value::Symbol(name.into()),
                ],
                Limits::default(),
            )?
            .value;
        let ir = self
            .compiler
            .invoke_refs(&[&source], Limits::default())?
            .value;
        Ok((source, Native::compile(&ir)?))
    }
    pub fn workbench(&self, text: &str) -> Result<String, Box<dyn std::error::Error>> {
        let (source, program) = self.link(text, "dock", "behavior")?;
        portable(&source)?;
        // A shape/type probe, not proof of all future inputs. Guest preview is separate.
        let probe = program.invoke(
            &[Value::Int(0), Value::Int(0), Value::Int(1)],
            Limits::default(),
        )?;
        if !matches!(probe.value, Value::Int(_)) {
            return Err("workbench behavior must return an integer".into());
        }
        let adapter = self.read(include_str!(
            "../../agel-stdlib/native-workbench-adapter.agel"
        ))?;
        let adapter = Native::compile(&self.compiler.invoke(&adapter, Limits::default())?.value)?;
        let definition = adapter
            .invoke(&[source], Limits::default())?
            .value
            .to_string();
        let preview =
            format!(":preview (begin {definition} (agent-become dock behavior) (activate))");
        if preview.len() > 256 {
            return Err("expanded behavior exceeds OS command limit".into());
        }
        Ok(definition)
    }
}

fn portable(value: &Value) -> Result<(), Box<dyn std::error::Error>> {
    match value {
        Value::Int(_) | Value::Bool(_) | Value::Nil => Ok(()),
        Value::Symbol(s)
            if !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_+-*/<=>?!.-".contains(&b)) =>
        {
            Ok(())
        }
        Value::List(xs) => {
            for x in xs {
                portable(x)?;
            }
            Ok(())
        }
        _ => {
            Err("OS source bridge accepts scalar/function syntax, not text or opaque values".into())
        }
    }
}

#[cfg(not(test))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tools = Tools::bootstrap()?;
    if std::env::args().nth(1).as_deref() == Some("--workbench") {
        let mut text = String::new();
        io::stdin().take(65537).read_to_string(&mut text)?;
        if text.len() > 65536 {
            return Err("source exceeds 65536 bytes".into());
        }
        println!("{}", tools.workbench(&text)?);
    } else {
        let (source, program) = tools.link(PACKAGE, "dock", "behavior")?;
        println!("Linked behavior: {source}");
        let result = program.invoke(
            &[Value::Int(0), Value::Int(40), Value::Int(1)],
            Limits::default(),
        )?;
        assert_eq!(result.value, Value::Int(42));
        println!("PASS: native Agel reader -> module linker/macro expansion -> compiler -> 42");
        println!("OS definition: {}", tools.workbench(PACKAGE)?);
    }
    Ok(())
}
