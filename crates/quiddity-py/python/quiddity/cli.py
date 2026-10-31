"""The Rust command line, with unchanged arguments, output and exit codes."""
import sys
from ._native import cli


def run():
    raise SystemExit(cli(sys.argv[1:]))
