import sys

from cli.app import main
from cli.models import CliError


if __name__ == "__main__":
    try:
        sys.exit(main())
    except CliError as exc:
        print(f"Error: {exc}", file=sys.stderr)
        sys.exit(1)