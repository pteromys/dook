#!/usr/bin/env python

import dataclasses
import functools
import typing


def identity(f):
    return f


# yeehaw
@dataclasses.dataclass
class one:
    two: int = 52
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
        yield


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
