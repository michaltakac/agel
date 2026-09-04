//! Agel's standard library, written in Agel and installed atomically.

use agel_core::{Commit, EvaluationOptions, TransactionError, World};

pub const SOURCE: &str = include_str!("../stdlib.agel");

pub fn install(world: &mut World, options: &EvaluationOptions) -> Result<Commit, TransactionError> {
    world.evaluate_with(SOURCE, options)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agel_core::Value;

    fn installed() -> World {
        let mut world = World::default();
        install(&mut world, &EvaluationOptions::default()).unwrap();
        world
    }

    #[test]
    fn sequence_and_result_libraries_are_agel_code() {
        let mut world = installed();
        let values = world
            .evaluate(
                "(import agel/sequence)
                 (import agel/result)
                 (list
                   (map (fn (x) (* x x)) '(1 2 3 4))
                   (filter (fn (x) (= x 2)) '(1 2 3))
                   (foldl + 0 '(1 2 3 4))
                   (unwrap-or (err 'nope) 99))",
            )
            .unwrap()
            .values;
        assert_eq!(
            values.last(),
            Some(&Value::List(vec![
                Value::List(vec![
                    Value::Int(1),
                    Value::Int(4),
                    Value::Int(9),
                    Value::Int(16)
                ]),
                Value::List(vec![Value::Int(2)]),
                Value::Int(10),
                Value::Int(99),
            ]))
        );
    }

    #[test]
    fn worker_pool_distributes_tasks_round_robin() {
        let mut world = installed();
        let result = world
            .evaluate(
                "(import agel/swarm)
                 (def observer (spawn \"observer\"))
                 (def left (make-worker \"left\" (fn (x) (+ x 100))))
                 (def right (make-worker \"right\" (fn (x) (+ x 200))))
                 (def pool (make-pool \"pool\" (list left right)))
                 (submit pool observer 1)
                 (submit pool observer 2)
                 (submit pool observer 3)
                 (run 10)
                 (list (recv observer) (recv observer) (recv observer)
                       (get (agent-info pool) 'heap))",
            )
            .unwrap()
            .values
            .pop()
            .unwrap();
        assert_eq!(
            result,
            Value::List(vec![
                Value::List(vec![
                    Value::Symbol("result".into()),
                    Value::Agent(2),
                    Value::Int(101),
                ]),
                Value::List(vec![
                    Value::Symbol("result".into()),
                    Value::Agent(3),
                    Value::Int(202),
                ]),
                Value::List(vec![
                    Value::Symbol("result".into()),
                    Value::Agent(2),
                    Value::Int(103),
                ]),
                Value::List(vec![Value::Agent(3), Value::Agent(2)]),
            ])
        );
    }

    #[test]
    fn empty_pool_fails_before_spawning() {
        let mut world = installed();
        let error = world
            .evaluate("(import agel/swarm) (make-pool \"empty\" nil)")
            .unwrap_err();
        assert!(error.to_string().contains("swarm/no-workers"));
    }
}
