"""
Tiny arithmetic evaluator for MagpieFX texture size expressions.

Magpie evaluates ``//!WIDTH`` / ``//!HEIGHT`` with muparser. The effects in
the wild only use constants such as ``INPUT_WIDTH`` combined with ``+ - * /``
and parentheses, but the common muparser operators and functions are
supported as well. Nothing is passed to Python's ``eval``.

Grammar (lowest to highest precedence)::

    expr    := cmp ('?' expr ':' expr)?
    cmp     := sum (('<' | '>' | '<=' | '>=' | '==' | '!=') sum)*
    sum     := term (('+' | '-') term)*
    term    := unary (('*' | '/' | '%') unary)*
    unary   := ('+' | '-') unary | power
    power   := atom ('^' unary)?
    atom    := NUMBER | NAME | NAME '(' args ')' | '(' expr ')'
"""

from __future__ import annotations

import math
import re
from typing import Callable, Dict, List, Mapping, Tuple

from .parser import MagpieFXError

_TOKEN_RE = re.compile(
    r"\s*(?:(\d+\.?\d*(?:[eE][+-]?\d+)?|\.\d+(?:[eE][+-]?\d+)?)"
    r"|([A-Za-z_]\w*)|(<=|>=|==|!=|[-+*/%^()?:,<>]))"
)

_FUNCS: Dict[str, Callable[..., float]] = {
    "min": min,
    "max": max,
    "abs": abs,
    "floor": math.floor,
    "ceil": math.ceil,
    "round": lambda x: math.floor(x + 0.5),
    "rint": lambda x: float(round(x)),
    "sqrt": math.sqrt,
    "log2": math.log2,
    "ln": math.log,
    "log": math.log,
    "exp": math.exp,
    "sign": lambda x: (x > 0) - (x < 0),
}


def _tokenize(text: str) -> List[Tuple[str, str]]:
    tokens: List[Tuple[str, str]] = []
    pos = 0
    text = text.strip()
    while pos < len(text):
        m = _TOKEN_RE.match(text, pos)
        if not m or m.end() == pos:
            raise MagpieFXError(f"Bad character in expression {text!r} at {pos}")
        num, name, op = m.groups()
        if num is not None:
            tokens.append(("num", num))
        elif name is not None:
            tokens.append(("name", name))
        else:
            tokens.append(("op", op))
        pos = m.end()
    tokens.append(("end", ""))
    return tokens


class _Parser:
    def __init__(self, text: str, variables: Mapping[str, float]) -> None:
        self.text = text
        self.tokens = _tokenize(text)
        self.pos = 0
        self.vars = variables

    def _peek(self) -> Tuple[str, str]:
        return self.tokens[self.pos]

    def _accept(self, op: str) -> bool:
        if self.tokens[self.pos] == ("op", op):
            self.pos += 1
            return True
        return False

    def _expect(self, op: str) -> None:
        if not self._accept(op):
            raise MagpieFXError(f"Expected '{op}' in expression {self.text!r}")

    def parse(self) -> float:
        value = self._expr()
        if self._peek()[0] != "end":
            raise MagpieFXError(f"Trailing tokens in expression {self.text!r}")
        return value

    def _expr(self) -> float:
        cond = self._cmp()
        if self._accept("?"):
            a = self._expr()
            self._expect(":")
            b = self._expr()
            return a if cond else b
        return cond

    def _cmp(self) -> float:
        value = self._sum()
        ops = {
            "<": lambda a, b: a < b,
            ">": lambda a, b: a > b,
            "<=": lambda a, b: a <= b,
            ">=": lambda a, b: a >= b,
            "==": lambda a, b: a == b,
            "!=": lambda a, b: a != b,
        }
        while self._peek()[0] == "op" and self._peek()[1] in ops:
            op = self._peek()[1]
            self.pos += 1
            value = float(ops[op](value, self._sum()))
        return value

    def _sum(self) -> float:
        value = self._term()
        while True:
            if self._accept("+"):
                value += self._term()
            elif self._accept("-"):
                value -= self._term()
            else:
                return value

    def _term(self) -> float:
        value = self._unary()
        while True:
            if self._accept("*"):
                value *= self._unary()
            elif self._accept("/"):
                d = self._unary()
                if d == 0:
                    raise MagpieFXError(f"Division by zero in {self.text!r}")
                value /= d
            elif self._accept("%"):
                value = math.fmod(value, self._unary())
            else:
                return value

    def _unary(self) -> float:
        if self._accept("-"):
            return -self._unary()
        if self._accept("+"):
            return self._unary()
        return self._power()

    def _power(self) -> float:
        base = self._atom()
        if self._accept("^"):
            return base ** self._unary()
        return base

    def _atom(self) -> float:
        kind, value = self._peek()
        self.pos += 1
        if kind == "num":
            return float(value)
        if kind == "name":
            if self._accept("("):
                func = _FUNCS.get(value)
                if func is None:
                    raise MagpieFXError(f"Unknown function '{value}' in {self.text!r}")
                args = [self._expr()]
                while self._accept(","):
                    args.append(self._expr())
                self._expect(")")
                return float(func(*args))
            if value not in self.vars:
                raise MagpieFXError(f"Unknown constant '{value}' in {self.text!r}")
            return float(self.vars[value])
        if (kind, value) == ("op", "("):
            result = self._expr()
            self._expect(")")
            return result
        raise MagpieFXError(f"Unexpected token {value!r} in expression {self.text!r}")


def evaluate(text: str, variables: Mapping[str, float]) -> float:
    """Evaluate *text* with the given named constants."""
    return _Parser(text, variables).parse()


def evaluate_size(text: str, variables: Mapping[str, float]) -> int:
    """Evaluate a size expression and round it like Magpie (``lround``)."""
    value = evaluate(text, variables)
    size = int(math.floor(value + 0.5))
    if size <= 0:
        raise MagpieFXError(f"Expression {text!r} gives non-positive size {value}")
    return size
