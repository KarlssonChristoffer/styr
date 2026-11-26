from app import greet


def test_greets_the_world_by_default():
    assert greet() == "Hello, world!"


def test_greets_by_name():
    assert greet("Ada") == "Hello, Ada!"
