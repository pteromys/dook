#!/usr/bin/env python

import dataclasses
import functools
import typing


def identity(f):
    return f


# yeehaw
@dataclasses.dataclass
class one:
    two: int = 52  # this is comment goes with two but not three
    three: int

    # this is a comment
    @identity
    @functools.cached_property
    # this is another comment
    def four(self, five=False):
        return (1, 2,
                3)

    # this is a comment
    def six(self,
            nine: dict[str, str] = {}, ten: dict[str, str] = {
                'a': 1,
                'b': 2,
            },
            dummy=None,
            ) -> None:
        yield functools.some.module.hecks(self, nine, ten, dummy)


def hecks(*yargs):
    return yargs


# this is a comment
# with multiple lines
# whee
seven = {
    'abc': ['a', 'b', 'c'],
    'xyz': ['x', 'y', 'z'],
}

eight = None


# i hope i guessed right that you care about string dictionary keys
seven['def'] = ['d', 'e', 'f']


def factorial(n: int) -> int:
    return permutations(n, n)


def permutations(n: int, k: int) -> int:
    if k <= 1:
        return 1
    return n * permutations(n - 1, k - 1)


def combinations(n: int, k: int) -> float:
    return permutations(n, k) / factorial(n - k)


def combinations2(n: int, k: int) -> float:
    return factorial(n) / factorial(k) / factorial(n - k)

# also try to catch setattr and friends
def shenanigans(x):
    setattr(x, 'attr', 1)  # yes 🦆
    object.__setattr__(x, 'attr', 2)  # yes 🦆
    x.__setitem__('attr', 3)  # yes 🦆
    dict.__setitem__(x, 'attr', 4)  # yes 🦆
    setattr('attr', 'nope', 5)  # no!!1 🪿
    object.__setattr__(x, 'attr')  # I mean this would throw if you actually ran it
