use agel_core::{Budget, EvaluationOptions, Value, World};

fn options() -> EvaluationOptions {
    EvaluationOptions {
        budget: Budget {
            fuel: 20_000_000,
            ..Budget::default()
        },
        ..EvaluationOptions::default()
    }
}

#[test]
fn agel_frames_recover_after_every_torn_sector_prefix() {
    let mut world = World::default();
    world
        .evaluate(include_str!("../journal-frame.agel"))
        .unwrap();
    let setup = world
        .evaluate_with(
            "(import agel/journal-frame) (def later (car (journal-pack \"later\")))
         (car (journal-pack \"original\"))",
            &options(),
        )
        .unwrap();
    let Value::String(frame) = setup.values.last().unwrap() else {
        panic!("no frame");
    };
    assert_eq!(frame.len(), 512);
    for cut in 0..=512 {
        let mut torn = frame.as_bytes().to_vec();
        torn[cut..].fill(0);
        let text = String::from_utf8(torn).unwrap();
        let call = format!(
            "(get (journal-read (text-concat {} later)) 'records)",
            Value::String(text)
        );
        let actual = world
            .evaluate_with(&call, &options())
            .unwrap()
            .values
            .pop()
            .unwrap();
        let expected = if cut == 512 {
            vec![
                Value::String("original".into()),
                Value::String("later".into()),
            ]
        } else {
            vec![Value::String("later".into())]
        };
        assert_eq!(actual, Value::List(expected), "cut {cut}");
    }
}

#[test]
fn utf8_and_incomplete_records_survive_framing() {
    let mut world = World::default();
    world
        .evaluate(include_str!("../journal-frame.agel"))
        .unwrap();
    let text = "Agel hrá RuneScape 🦀\n".repeat(40);
    let call = format!(
        "(import agel/journal-frame)
      (def join (fn (xs out) (if (= xs nil) out (join (cdr xs) (text-concat out (car xs))))))
      (def packed (journal-pack {}))
      (def whole (join packed \"\"))
      (get (journal-read whole) 'records)",
        Value::String(text.clone())
    );
    let result = world
        .evaluate_with(&call, &options())
        .unwrap()
        .values
        .pop()
        .unwrap();
    assert_eq!(result, Value::List(vec![Value::String(text)]));
    let result = world.evaluate_with(
        "(get (journal-read (text-concat (car packed) (car (journal-pack \"recovered\")))) 'records)",
        &options()).unwrap().values.pop().unwrap();
    assert_eq!(result, Value::List(vec![Value::String("recovered".into())]));
    assert!(world.evaluate("(text-from-bytes '(255))").is_err());
    assert!(world.evaluate("(text-from-bytes '(256))").is_err());
    assert_eq!(
        world.evaluate("(text-from-bytes nil)").unwrap().values,
        vec![Value::String(String::new())]
    );
}
