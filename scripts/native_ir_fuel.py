"""Independent, test-only interpreter for the integer backend's IR fuel model.

One unit per evaluated node, including a call's callee and function creation.
Entry arguments and external invocation are not IR nodes. Deliberately uses
ordinary closures/calls: it knows nothing about x86 registers or inlining.
"""
from dataclasses import dataclass


class Exhausted(Exception):
    pass


@dataclass
class Closure:
    arity: int
    body: list
    scopes: list


class Interpreter:
    def __init__(self, fuel):
        self.remaining = fuel

    def evaluate(self, node, scopes):
        if self.remaining == 0:
            raise Exhausted()
        self.remaining -= 1
        kind, *parts = node
        if kind == 'const':
            return parts[0]
        if kind == 'local':
            return scopes[parts[0]][parts[1]]
        if kind == 'builtin':
            return parts[0]
        if kind == 'fn':
            return Closure(*parts, scopes)
        if kind == 'if':
            condition = self.evaluate(parts[0], scopes)
            return self.evaluate(parts[2] if condition is False or condition is None else parts[1], scopes)
        if kind == 'begin':
            result = None
            for form in parts:
                result = self.evaluate(form, scopes)
            return result
        if kind in ('call', 'tail-call'):
            callee = self.evaluate(parts[0], scopes)
            arguments = [self.evaluate(arg, scopes) for arg in parts[1]]
            return self.apply(callee, arguments)
        raise AssertionError(f'unknown node: {node}')

    def apply(self, callee, arguments):
        if isinstance(callee, Closure):
            assert len(arguments) == callee.arity
            return self.evaluate(callee.body, [arguments, *callee.scopes])
        left, right = arguments
        if callee == '+':
            return left + right
        if callee == '-':
            return left - right
        if callee == '*':
            return left * right
        if callee == '/':
            # Integer division truncates toward zero, without floating point.
            quotient = abs(left) // abs(right)
            return -quotient if (left < 0) != (right < 0) else quotient
        if callee == '=':
            return left == right
        if callee == '<':
            return left < right
        raise AssertionError(f'unknown primitive: {callee}')

    def run(self, ir, arguments):
        assert ir[0] == 'agel/native-v2'
        callee = self.evaluate(ir[1], [])
        if callee.arity == len(arguments) + 1:
            arguments = [callee, *arguments]
        return self.apply(callee, arguments)


def form(value):
    if isinstance(value, list):
        return '(' + ' '.join(map(form, value)) + ')'
    if value is None:
        return 'nil'
    if value is True:
        return '#t'
    if value is False:
        return '#f'
    return str(value)


def const(n):
    return ['const', n]


def local(slot, depth=0):
    return ['local', depth, slot]


def call(op, *args):
    return ['call', ['builtin', op], list(args)]


def cases():
    """Name, IR, entry arguments. Cover optimized and ordinary evaluation."""
    body = ['if', call('=', local(1), const(0)), local(2),
            ['tail-call', local(0), [local(0), call('-', local(1), const(1)),
                                    call('+', local(2), local(1))]]]
    recursive = ['if', call('=', local(1), const(0)), const(0),
                 call('+', ['call', local(0), [local(0), call('-', local(1), const(1))]], const(1))]
    inline = ['call', ['fn', 1, call('*', local(0), const(2))], [call('+', const(20), const(1))]]
    # The last call belongs to an inline lambda, but is an operand in the
    # enclosing physical frame: it must return and preserve the shared fuel.
    nested = call('+', ['call', ['fn', 1, ['tail-call', local(0), [const(40)]]],
                               [['fn', 1, call('+', local(0), const(1))]]], const(1))
    fixtures = [
        ('constant', ['fn', 0, const(42)], []),
        ('empty-begin', ['fn', 0, ['begin']], []),
        ('comparison', ['fn', 0, call('<', const(20), const(22))], []),
        ('nil-branch', ['fn', 0, ['if', const(None), const(7), const(42)]], []),
        ('zero-truthy', ['fn', 0, ['if', const(0), const(42), const(7)]], []),
        ('primitive', ['fn', 0, call('+', const(20), const(22))], []),
        ('branch-true', ['fn', 0, ['if', const(True), const(42), call('/', const(1), const(0))]], []),
        ('branch-false', ['fn', 0, ['if', const(False), call('/', const(1), const(0)), const(42)]], []),
        ('begin', ['fn', 0, ['begin', const(7), call('+', const(20), const(22))]], []),
        ('inline', ['fn', 0, inline], []),
        ('nested', ['fn', 0, nested], []),
        ('recursion', ['fn', 2, recursive], [4]),
        ('tail-loop', ['fn', 3, body], [5, 0]),
        ('negative', ['fn', 0, call('/', const(-85), const(2))], []),
    ]
    return [(name, ['agel/native-v2', fn], args) for name, fn, args in fixtures]
