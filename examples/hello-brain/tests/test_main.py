"""Trivial tests for the hello-brain example. Gives the test-health
domain something to observe.
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent / "src"))

import pytest
from main import greet


def test_greet_returns_greeting():
    assert greet("Alice") == "Hello, Alice!"


def test_greet_handles_unicode():
    assert greet("世界") == "Hello, 世界!"


def test_greet_raises_on_empty():
    with pytest.raises(ValueError):
        greet("")
