import sys

from app import greet


def main(argv: list[str]) -> int:
    print(greet(argv[1]) if len(argv) > 1 else greet())
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
