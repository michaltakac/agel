"""Closure lifetime regressions for guest code and the independent interpreter."""
from native_ir_fuel import call, const, local


def invoke(callee, *args):
    return ['call', callee, list(args)]


def ir(body, arity=0):
    return ['agel/native-v2', ['fn', arity, body]]


def cases():
    factory = ['fn', 1, ['fn', 1, call('+', local(0), local(0, 1))]]
    # Return from an ordinary factory call, then call the returned closure.
    escaped = invoke(['fn', 1, invoke(invoke(local(0), const(40)), const(2))], factory)
    # Copy an already captured value into a new closure, then return again.
    three = ['fn', 1, ['fn', 1, ['fn', 1,
             call('+', local(0), call('+', local(0, 1), local(0, 2)))]]]
    transitive = invoke(invoke(invoke(three, const(20)), const(10)), const(12))
    # A tail call replaces the frame whose value the argument just captured.
    consumer = ['fn', 1, invoke(local(0), const(2))]
    capturing_argument = ['fn', 1, call('+', local(0), local(0, 2))]
    tail = invoke(['fn', 1, ['tail-call', local(0), [capturing_argument]]], consumer)
    siblings = invoke(['fn', 2, call('+', invoke(local(0), const(2)), invoke(local(1), const(2)))],
                      invoke(factory, const(40)), invoke(factory, const(41)))
    return [
        ('escaped-frame', ir(escaped), []),
        ('transitive-capture', ir(transitive), []),
        ('capture-through-tail', ir(tail, 1), [40]),
        ('independent-captures', ir(siblings), []),
    ]


def limits():
    """IR, arguments, arena bytes, status, printed result; exact physical sizes."""
    factory = ['fn', 1, ['fn', 1, call('+', local(0), local(0, 1))]]
    escaped_inline = ir(invoke(invoke(factory, const(40)), const(2)))
    allocating = ir(['if', call('=', local(1), const(0)), const(42),
                     ['begin', ['fn', 0, const(0)],
                      ['tail-call', local(0), [local(0), call('-', local(1), const(1))]]]], 2)
    wrong_arity = ir(invoke(['fn', 1, invoke(local(0))], ['fn', 1, local(0)]))
    return [
        (ir(const(42)), [], 0, 113, None),
        (ir(const(42)), [], 15, 113, None),
        (ir(const(42)), [], 16, 42, 42),
        (escaped_inline, [], 39, 113, None),
        (escaped_inline, [], 40, 42, 42),
        (allocating, [3], 111, 113, None),
        (allocating, [3], 112, 42, 42),
        (ir(['fn', 0, const(42)]), [], 1024, 114, None),
        (ir(invoke(const(42))), [], 1024, 114, None),
        (wrong_arity, [], 1024, 114, None),
        (ir(['tail-call', local(0), [local(0)]], 2), [0], 1024, 114, None),
        (ir(call('+', ['fn', 0, const(1)], const(2))), [], 1024, 114, None),
        (ir(call('*', const(True), const(2))), [], 1024, 114, None),
    ]
