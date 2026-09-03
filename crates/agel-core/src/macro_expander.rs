use crate::Expr;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MacroDef {
    pub params: Vec<String>,
    pub template: Expr,
    pub definition_module: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExpansionError {
    Invalid(String),
    Limit(String),
}

pub(crate) fn expand(
    definition: &MacroDef,
    arguments: &[Expr],
    next_syntax_id: &mut u64,
    max_nodes: u64,
    max_collection_len: usize,
) -> Result<(Expr, u64), ExpansionError> {
    if arguments.len() != definition.params.len() {
        return Err(ExpansionError::Invalid(format!(
            "macro expects {} argument(s), got {}",
            definition.params.len(),
            arguments.len()
        )));
    }
    let substitutions = definition
        .params
        .iter()
        .cloned()
        .zip(arguments.iter().cloned())
        .collect::<BTreeMap<_, _>>();
    let nodes = measure_template(
        &definition.template,
        &substitutions,
        max_nodes,
        max_collection_len,
    )?;
    let expanded = expand_template(
        &definition.template,
        &substitutions,
        &BTreeMap::new(),
        &definition.definition_module,
        next_syntax_id,
    )
    .map_err(ExpansionError::Invalid)?;
    Ok((expanded, nodes))
}

fn measure_template(
    expression: &Expr,
    substitutions: &BTreeMap<String, Expr>,
    max_nodes: u64,
    max_collection_len: usize,
) -> Result<u64, ExpansionError> {
    fn walk(
        expression: &Expr,
        substitutions: Option<&BTreeMap<String, Expr>>,
        remaining: &mut u64,
        max_collection_len: usize,
    ) -> Result<(), ExpansionError> {
        if *remaining == 0 {
            return Err(ExpansionError::Limit(
                "macro expansion exceeds remaining fuel".into(),
            ));
        }
        *remaining -= 1;
        if let Expr::Symbol(name) = expression {
            if let Some(argument) = substitutions.and_then(|values| values.get(name)) {
                *remaining = remaining.checked_add(1).expect("one node was consumed");
                return walk(argument, None, remaining, max_collection_len);
            }
        }
        if let Expr::List(items) = expression {
            if items.len() > max_collection_len {
                return Err(ExpansionError::Limit(format!(
                    "macro output list length {} exceeds limit {max_collection_len}",
                    items.len()
                )));
            }
            let quoted = matches!(items.first(), Some(Expr::Symbol(name)) if name == "quote");
            for item in items {
                walk(
                    item,
                    if quoted { None } else { substitutions },
                    remaining,
                    max_collection_len,
                )?;
            }
        }
        Ok(())
    }

    let mut remaining = max_nodes;
    walk(
        expression,
        Some(substitutions),
        &mut remaining,
        max_collection_len,
    )?;
    Ok(max_nodes - remaining)
}

fn expand_template(
    expression: &Expr,
    substitutions: &BTreeMap<String, Expr>,
    renames: &BTreeMap<String, String>,
    definition_module: &Option<String>,
    next_syntax_id: &mut u64,
) -> Result<Expr, String> {
    match expression {
        Expr::Symbol(name) => {
            if let Some(argument) = substitutions.get(name) {
                return Ok(argument.clone());
            }
            if let Some(renamed) = renames.get(name) {
                return Ok(Expr::Symbol(renamed.clone()));
            }
            if is_core_syntax(name) {
                Ok(Expr::Symbol(name.clone()))
            } else {
                Ok(Expr::ScopedSymbol {
                    name: name.clone(),
                    module: definition_module.clone(),
                })
            }
        }
        Expr::List(items) if matches!(items.first(), Some(Expr::Symbol(name)) if name == "quote") => {
            Ok(expression.clone())
        }
        Expr::List(items) if matches!(items.first(), Some(Expr::Symbol(name)) if name == "fn") => {
            expand_function(
                items,
                substitutions,
                renames,
                definition_module,
                next_syntax_id,
            )
        }
        Expr::List(items) if matches!(items.first(), Some(Expr::Symbol(name)) if name == "let") => {
            expand_let(
                items,
                substitutions,
                renames,
                definition_module,
                next_syntax_id,
            )
        }
        Expr::List(items) if matches!(items.first(), Some(Expr::Symbol(name)) if name == "with-handler") => {
            expand_handler(
                items,
                substitutions,
                renames,
                definition_module,
                next_syntax_id,
            )
        }
        Expr::List(items) if matches!(items.first(), Some(Expr::Symbol(name)) if name == "with-restart") => {
            expand_restart(
                items,
                substitutions,
                renames,
                definition_module,
                next_syntax_id,
            )
        }
        Expr::List(items) => items
            .iter()
            .map(|item| {
                expand_template(
                    item,
                    substitutions,
                    renames,
                    definition_module,
                    next_syntax_id,
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Expr::List),
        Expr::Nil | Expr::Bool(_) | Expr::Int(_) | Expr::String(_) | Expr::ScopedSymbol { .. } => {
            Ok(expression.clone())
        }
    }
}

fn expand_function(
    items: &[Expr],
    substitutions: &BTreeMap<String, Expr>,
    renames: &BTreeMap<String, String>,
    definition_module: &Option<String>,
    next_syntax_id: &mut u64,
) -> Result<Expr, String> {
    let Some(Expr::List(params)) = items.get(1) else {
        return Err("macro template fn requires a parameter list".into());
    };
    let mut body_renames = renames.clone();
    let mut expanded_params = Vec::with_capacity(params.len());
    for param in params {
        let Expr::Symbol(name) = param else {
            return Err("macro template fn parameters must be symbols".into());
        };
        let fresh = fresh_name(name, next_syntax_id)?;
        body_renames.insert(name.clone(), fresh.clone());
        expanded_params.push(Expr::Symbol(fresh));
    }
    let mut result = vec![Expr::Symbol("fn".into()), Expr::List(expanded_params)];
    for item in &items[2..] {
        result.push(expand_template(
            item,
            substitutions,
            &body_renames,
            definition_module,
            next_syntax_id,
        )?);
    }
    Ok(Expr::List(result))
}

fn expand_let(
    items: &[Expr],
    substitutions: &BTreeMap<String, Expr>,
    renames: &BTreeMap<String, String>,
    definition_module: &Option<String>,
    next_syntax_id: &mut u64,
) -> Result<Expr, String> {
    let Some(Expr::List(bindings)) = items.get(1) else {
        return Err("macro template let requires a binding list".into());
    };
    let mut body_renames = renames.clone();
    let mut expanded_bindings = Vec::with_capacity(bindings.len());
    for binding in bindings {
        let Expr::List(pair) = binding else {
            return Err("macro template let bindings must be pairs".into());
        };
        if pair.len() != 2 {
            return Err("macro template let bindings must be pairs".into());
        }
        let Expr::Symbol(name) = &pair[0] else {
            return Err("macro template let binding names must be symbols".into());
        };
        let value = expand_template(
            &pair[1],
            substitutions,
            renames,
            definition_module,
            next_syntax_id,
        )?;
        let fresh = fresh_name(name, next_syntax_id)?;
        body_renames.insert(name.clone(), fresh.clone());
        expanded_bindings.push(Expr::List(vec![Expr::Symbol(fresh), value]));
    }
    let mut result = vec![Expr::Symbol("let".into()), Expr::List(expanded_bindings)];
    for item in &items[2..] {
        result.push(expand_template(
            item,
            substitutions,
            &body_renames,
            definition_module,
            next_syntax_id,
        )?);
    }
    Ok(Expr::List(result))
}

fn expand_handler(
    items: &[Expr],
    substitutions: &BTreeMap<String, Expr>,
    renames: &BTreeMap<String, String>,
    definition_module: &Option<String>,
    next_syntax_id: &mut u64,
) -> Result<Expr, String> {
    if items.len() < 4 {
        return Err("macro template with-handler is incomplete".into());
    }
    let Expr::List(spec) = &items[1] else {
        return Err("macro template with-handler requires a handler spec".into());
    };
    if spec.len() != 2 {
        return Err("macro template handler spec must contain kind and variable".into());
    }
    let Expr::Symbol(variable) = &spec[1] else {
        return Err("macro template handler variable must be a symbol".into());
    };
    let kind = expand_template(
        &spec[0],
        substitutions,
        renames,
        definition_module,
        next_syntax_id,
    )?;
    let fresh = fresh_name(variable, next_syntax_id)?;
    let mut handler_renames = renames.clone();
    handler_renames.insert(variable.clone(), fresh.clone());
    let handler = expand_template(
        &items[2],
        substitutions,
        &handler_renames,
        definition_module,
        next_syntax_id,
    )?;
    let mut result = vec![
        Expr::Symbol("with-handler".into()),
        Expr::List(vec![kind, Expr::Symbol(fresh)]),
        handler,
    ];
    for body in &items[3..] {
        result.push(expand_template(
            body,
            substitutions,
            renames,
            definition_module,
            next_syntax_id,
        )?);
    }
    Ok(Expr::List(result))
}

fn expand_restart(
    items: &[Expr],
    substitutions: &BTreeMap<String, Expr>,
    renames: &BTreeMap<String, String>,
    definition_module: &Option<String>,
    next_syntax_id: &mut u64,
) -> Result<Expr, String> {
    if items.len() < 4 {
        return Err("macro template with-restart is incomplete".into());
    }
    let Expr::List(spec) = &items[1] else {
        return Err("macro template with-restart requires a restart spec".into());
    };
    let Some(name) = spec.first() else {
        return Err("macro template restart spec cannot be empty".into());
    };
    let expanded_name = expand_template(
        name,
        substitutions,
        renames,
        definition_module,
        next_syntax_id,
    )?;
    let mut handler_renames = renames.clone();
    let mut expanded_spec = vec![expanded_name];
    for param in &spec[1..] {
        let Expr::Symbol(name) = param else {
            return Err("macro template restart parameters must be symbols".into());
        };
        let fresh = fresh_name(name, next_syntax_id)?;
        handler_renames.insert(name.clone(), fresh.clone());
        expanded_spec.push(Expr::Symbol(fresh));
    }
    let handler = expand_template(
        &items[2],
        substitutions,
        &handler_renames,
        definition_module,
        next_syntax_id,
    )?;
    let mut result = vec![
        Expr::Symbol("with-restart".into()),
        Expr::List(expanded_spec),
        handler,
    ];
    for body in &items[3..] {
        result.push(expand_template(
            body,
            substitutions,
            renames,
            definition_module,
            next_syntax_id,
        )?);
    }
    Ok(Expr::List(result))
}

fn fresh_name(name: &str, next_syntax_id: &mut u64) -> Result<String, String> {
    let id = *next_syntax_id;
    *next_syntax_id = next_syntax_id
        .checked_add(1)
        .ok_or_else(|| "syntax identifier space exhausted".to_owned())?;
    Ok(format!("{name}#{id}"))
}

fn is_core_syntax(name: &str) -> bool {
    matches!(
        name,
        "quote"
            | "if"
            | "begin"
            | "def"
            | "fn"
            | "let"
            | "defmacro"
            | "defprotocol"
            | "macroexpand-1"
            | "module"
            | "export"
            | "import"
            | "with-handler"
            | "with-restart"
            | "invoke-restart"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn introduced_let_bindings_are_renamed_but_arguments_are_opaque() {
        let definition = MacroDef {
            params: vec!["value".into(), "body".into()],
            template: Expr::List(vec![
                Expr::Symbol("let".into()),
                Expr::List(vec![Expr::List(vec![
                    Expr::Symbol("tmp".into()),
                    Expr::Symbol("value".into()),
                ])]),
                Expr::Symbol("body".into()),
            ]),
            definition_module: None,
        };
        let mut id = 1;
        let (expanded, nodes) = expand(
            &definition,
            &[Expr::Int(10), Expr::Symbol("tmp".into())],
            &mut id,
            100,
            100,
        )
        .unwrap();
        assert_eq!(nodes, 7);
        assert_eq!(
            expanded,
            Expr::List(vec![
                Expr::Symbol("let".into()),
                Expr::List(vec![Expr::List(vec![
                    Expr::Symbol("tmp#1".into()),
                    Expr::Int(10),
                ])]),
                Expr::Symbol("tmp".into()),
            ])
        );
    }
}
