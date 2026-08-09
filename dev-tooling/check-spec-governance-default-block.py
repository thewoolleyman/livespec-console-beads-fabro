"""Verify this repo's documented spec_governance defaults block."""

from __future__ import annotations

import json
import sys
from pathlib import Path

from returns.primitives.exceptions import UnwrapFailedError

from livespec_runtime.spec_governance import verify_livespec_jsonc_default_block


def main() -> int:
    result = verify_livespec_jsonc_default_block(path=Path(".livespec.jsonc"))
    try:
        payload = result.unwrap()
    except UnwrapFailedError:
        print(result.failure(), file=sys.stderr)
        return 1
    print(json.dumps(payload, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
