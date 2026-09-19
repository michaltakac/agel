//! `agel/judgment`: the typed-judgment contract from the language's side.
//! A request the library prints is the form the host's provider reads; an
//! answer line from the provider or from the judge written in Agel parses
//! into the same groups.
use agel_core::{EvaluationOptions, Value, World};
use agel_model::JudgmentRequest;

fn world() -> World {
    let mut world = World::new(0);
    agel_stdlib::install(&mut world, &EvaluationOptions::default()).unwrap();
    world.evaluate("(import agel/judgment)").unwrap();
    world
}

fn last(world: &mut World, source: &str) -> Value {
    world
        .evaluate(source)
        .unwrap_or_else(|error| panic!("{source}: {error}"))
        .values
        .pop()
        .unwrap()
}

const QUESTIONS: &str = r#"(list
  '(choice "act" "Best next move" "forward" "back" "left" "right" ("fire" "shoot") "use")
  '(noul "foe" "Is an enemy in view?" "one is" "none is")
  '(score "risk" "How dangerous?" "safe" "wary" "lethal"))"#;

const REPLY: &str =
    "act choice 6 fire 330 320 15 220 10 440 0 foe noul 980 risk score 3 740 600 270 730 0";

#[test]
fn a_printed_request_is_the_form_the_host_reads() {
    let mut world = world();
    let text = last(
        &mut world,
        &format!(
            r#"(judgment-request (list (list "game" "DOOM E1M1") (list "note" "say \"hi\"\n")) {QUESTIONS})"#
        ),
    );
    let Value::String(text) = text else {
        panic!("{text}")
    };
    let request = JudgmentRequest::parse(&text).unwrap();
    let json = request.to_json("jev-latest");
    assert_eq!(json["state"]["game"], "DOOM E1M1");
    assert_eq!(json["state"]["note"], "say \"hi\"\n");
    assert_eq!(json["questions"]["act"]["criteria"]["fire"], "shoot");
    assert!(json["questions"]["act"]["criteria"]["use"].is_null());
    assert_eq!(json["questions"]["foe"]["criteria"]["false"], "none is");
    assert_eq!(json["questions"]["risk"]["criteria"][1], "wary");
    let plain = last(
        &mut world,
        r#"(judgment-request "a corridor" (list '(noul "q" "Is it dark?")))"#,
    );
    assert_eq!(
        plain,
        Value::String(r#"(judge "a corridor" (noul "q" "Is it dark?"))"#.into())
    );
    let stateless = last(
        &mut world,
        r#"(judgment-request nil (list '(noul "q" "?")))"#,
    );
    assert_eq!(stateless, Value::String(r#"(judge (noul "q" "?"))"#.into()));
    let error = world
        .evaluate(r#"(judgment-request nil (list '(guess "q" "?")))"#)
        .unwrap_err();
    assert!(error.to_string().contains("judgment/question"), "{error}");
}

#[test]
fn an_answer_line_parses_into_groups_with_accessors() {
    let mut world = world();
    let answers = last(&mut world, &format!("(judgment-parse \"{REPLY}\")"));
    assert_eq!(
        answers.to_string(),
        r#"(("act" choice "fire" 330 (320 15 220 10 440 0)) ("foe" noul 980 nil nil) ("risk" score 740 600 (270 730 0)))"#
    );
    world
        .evaluate(&format!("(def answers (judgment-parse \"{REPLY}\"))"))
        .unwrap();
    assert_eq!(
        last(&mut world, r#"(answer-value (answer answers "act"))"#),
        Value::String("fire".into())
    );
    assert_eq!(
        last(&mut world, r#"(answer-confidence (answer answers "act"))"#),
        Value::Int(330)
    );
    assert_eq!(
        last(&mut world, r#"(answer-kind (answer answers "foe"))"#),
        Value::Symbol("noul".into())
    );
    assert_eq!(
        last(&mut world, r#"(answer-value (answer answers "foe"))"#),
        Value::Int(980)
    );
    assert_eq!(
        last(
            &mut world,
            r#"(answer-probabilities (answer answers "risk"))"#
        )
        .to_string(),
        "(270 730 0)"
    );
    assert_eq!(last(&mut world, r#"(answer answers "none")"#), Value::Nil);
    for (line, reason) in [
        ("act guess 1", "noul, choice or score"),
        ("foe noul many", "not a number"),
        ("foe noul -", "not a number"),
    ] {
        let error = world
            .evaluate(&format!("(judgment-parse \"{line}\")"))
            .unwrap_err();
        assert!(error.to_string().contains(reason), "{line}: {error}");
    }
    assert_eq!(last(&mut world, "(read-int \"-42\")"), Value::Int(-42));
    assert_eq!(
        last(&mut world, "(int->text -1050)"),
        Value::String("-1050".into())
    );
    assert_eq!(
        last(&mut world, "(tokens \"  a bb  c \")").to_string(),
        r#"("a" "bb" "c")"#
    );
}

#[test]
fn the_judge_written_in_agel_answers_on_the_same_line() {
    let mut world = world();
    world
        .evaluate(&format!(
            r#"(def questions {QUESTIONS})
               (def rules (list
                 (list "act" "fire" 6 "imp")
                 (list "act" "back" 3 "health 2")
                 (list "foe" "yes" 8 "imp")
                 (list "risk" "lethal" 4 "imp")
                 (list "risk" "wary" 2 "imp")))"#
        ))
        .unwrap();
    let line = last(
        &mut world,
        r#"(judge-locally (list (list "frame" "an imp ahead") (list "state" "health 80")) questions rules)"#,
    );
    let Value::String(line) = line else {
        panic!("{line}")
    };
    // Weights 1 each plus 6 on fire: 12 in all; fire 583, the rest 83. The
    // levels weigh 1, 3 and 5 of 9: a score of 1443 thousandths of a level.
    assert_eq!(
        line,
        "act choice 6 fire 500 83 83 83 83 583 83 foe noul 900 risk score 3 1443 222 111 333 555"
    );
    world.evaluate(&format!("(def line \"{line}\")")).unwrap();
    assert_eq!(
        last(
            &mut world,
            r#"(answer-value (answer (judgment-parse line) "act"))"#
        ),
        Value::String("fire".into())
    );
    // Nothing matches: even answers, no confidence, the first option.
    let calm = last(
        &mut world,
        r#"(judge-locally "a quiet corridor" questions rules)"#,
    );
    assert_eq!(
        calm,
        Value::String(
            "act choice 6 forward 0 166 166 166 166 166 166 foe noul 500 risk score 3 999 0 333 333 333"
                .into()
        )
    );
    // The line is what the provider would say: the host reads it the same.
    let parsed = last(&mut world, "(judgment-parse line)");
    assert!(parsed
        .to_string()
        .starts_with(r#"(("act" choice "fire" 500"#));
}
